use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};

use thiserror::Error;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio::sync::oneshot::error::RecvError;
use tokio::sync::oneshot::{channel, Sender};
use tokio::task::JoinHandle;

#[derive(Debug)]
pub struct Observable<T: Clone> {
  sender: UnboundedSender<ObserverCommand<T>>,
  handle: JoinHandle<()>,
}

enum ObserverCommand<T: Clone> {
  GetValue(Sender<T>),
  UpdateValue(T, Sender<()>),
  AddObserver(UnboundedSender<(T, T, Sender<()>)>, Sender<usize>),
  RemoveObserver(usize, Sender<()>),
}

pub struct ObserverHandle<T: Clone> {
  id: usize,
  stop_on_drop: bool,
  sender: UnboundedSender<ObserverCommand<T>>,
}

#[derive(Debug, Error)]
pub enum ObserverError {
  #[error("Failed to send command to observable")]
  SendError,

  #[error("Failed to receive command answer from observable")]
  RecvError,
}

impl<T> From<SendError<T>> for ObserverError {
  fn from(_: SendError<T>) -> Self {
    ObserverError::SendError
  }
}

impl From<RecvError> for ObserverError {
  fn from(_: RecvError) -> Self {
    ObserverError::RecvError
  }
}

impl<T: Clone + 'static> Observable<T> {
  pub fn new_local(val: T) -> Self where {
    let (sender, mut receiver) = unbounded_channel::<ObserverCommand<T>>();
    let handle = tokio::task::spawn_local(async move {
      let mut value = val;
      let mut observers: HashMap<usize, UnboundedSender<(T, T, Sender<()>)>> =
        HashMap::new();

      while let Some(command) = receiver.recv().await {
        match command {
          ObserverCommand::GetValue(responder) => {
            let _ignored = responder.send(value.clone());
          }
          ObserverCommand::UpdateValue(new_value, responder) => {
            let old_value = value;
            value = new_value;
            for observer in observers.values() {
              let (sender, receiver) = channel();
              if let Ok(_) =
                observer.send((old_value.clone(), value.clone(), sender))
              {
                let _ignored = receiver.await;
              }
            }
            let _ignored = responder.send(());
          }
          ObserverCommand::AddObserver(observer, responder) => {
            static ID_COUNTER: AtomicUsize = AtomicUsize::new(1);
            let id = ID_COUNTER.fetch_add(1, Ordering::Relaxed);
            let _ignored = observers.insert(id, observer);
            let _ignored = responder.send(id);
          }
          ObserverCommand::RemoveObserver(id, responder) => {
            let _ignored = observers.remove(&id);
            let _ignored = responder.send(());
          }
        }
      }
    });

    Self { sender, handle }
  }

  pub fn new(val: T) -> Self
  where
    T: Sync + Send,
  {
    let (sender, mut receiver) = unbounded_channel::<ObserverCommand<T>>();
    let handle = tokio::spawn(async move {
      let mut value = val;
      let mut observers: HashMap<usize, UnboundedSender<(T, T, Sender<()>)>> =
        HashMap::new();

      while let Some(command) = receiver.recv().await {
        match command {
          ObserverCommand::GetValue(responder) => {
            let _ignored = responder.send(value.clone());
          }
          ObserverCommand::UpdateValue(new_value, responder) => {
            let old_value = value;
            value = new_value;
            for observer in observers.values() {
              let (sender, receiver) = channel();
              if observer
                .send((old_value.clone(), value.clone(), sender))
                .is_ok()
              {
                let _ignored = receiver.await;
              }
            }
            let _ignored = responder.send(());
          }
          ObserverCommand::AddObserver(observer, responder) => {
            static ID_COUNTER: AtomicUsize = AtomicUsize::new(1);
            let id = ID_COUNTER.fetch_add(1, Ordering::Relaxed);
            let _ignored = observers.insert(id, observer);
            let _ignored = responder.send(id);
          }
          ObserverCommand::RemoveObserver(id, responder) => {
            let _ignored = observers.remove(&id);
            let _ignored = responder.send(());
          }
        }
      }
    });

    Self { sender, handle }
  }

  pub async fn observe_local<Fut>(
    &self,
    fun: impl Fn(T, T) -> Fut + 'static,
  ) -> Result<ObserverHandle<T>, ObserverError>
  where
    Fut: Future<Output = ()>,
  {
    let (sender, mut observer) = unbounded_channel::<(T, T, Sender<()>)>();
    let (responder, response_receiver) = channel();
    let _ignored = self
      .sender
      .send(ObserverCommand::AddObserver(sender, responder))?;
    tokio::task::spawn_local(async move {
      while let Some((old, new, responder)) = observer.recv().await {
        fun(old, new).await;
        let _ignored = responder.send(());
      }
    });
    let id = response_receiver.await?;

    Ok(ObserverHandle {
      id,
      stop_on_drop: false,
      sender: self.sender.clone(),
    })
  }

  pub async fn observe<Fut>(
    &self,
    fun: impl Fn(T, T) -> Fut + Sync + Send + 'static,
  ) -> Result<ObserverHandle<T>, ObserverError>
  where
    Fut: Future<Output = ()> + Sync + Send,
    T: Send,
  {
    let (sender, mut observer) = unbounded_channel::<(T, T, Sender<()>)>();
    let (responder, response_receiver) = channel();
    let _ignored = self
      .sender
      .send(ObserverCommand::AddObserver(sender, responder))?;
    tokio::spawn(async move {
      while let Some((old, new, responder)) = observer.recv().await {
        fun(old, new).await;
        let _ignored = responder.send(());
      }
    });
    let id = response_receiver.await?;

    Ok(ObserverHandle {
      id,
      stop_on_drop: false,
      sender: self.sender.clone(),
    })
  }

  pub async fn update_wait(&self, new: T) -> Result<(), ObserverError> {
    let (responder, response) = channel();
    self
      .sender
      .send(ObserverCommand::UpdateValue(new, responder))?;
    response.await?;
    Ok(())
  }

  pub fn update(&self, new: T) -> Result<(), ObserverError> {
    let (responder, _) = channel();
    self
      .sender
      .send(ObserverCommand::UpdateValue(new, responder))?;
    Ok(())
  }

  pub async fn get_clone(&self) -> Result<T, ObserverError> {
    let (responder, response) = channel();
    let _ignored = self.sender.send(ObserverCommand::GetValue(responder))?;
    response.await.map_err(ObserverError::from)
  }
}

impl<T: Clone> Drop for Observable<T> {
  fn drop(&mut self) {
    self.handle.abort();
  }
}

impl<T: Clone> ObserverHandle<T> {
  pub async fn stop_observing(&self) -> Result<(), ObserverError> {
    let (responder, response) = channel();
    self
      .sender
      .send(ObserverCommand::RemoveObserver(self.id, responder))?;
    response.await?;
    Ok(())
  }

  pub fn stop_on_drop(mut self) -> Self {
    self.stop_on_drop = true;
    self
  }
}

impl<T: Clone> Drop for ObserverHandle<T> {
  fn drop(&mut self) {
    if self.stop_on_drop {
      let (responder, _) = channel();
      let _ignored = self
        .sender
        .send(ObserverCommand::RemoveObserver(self.id, responder));
    }
  }
}
