#![feature(fn_traits)]
#![feature(negative_impls)]

mod observer;
pub use observer::{Observable, ObserverError, ObserverHandle};

#[cfg(test)]
mod tests {
  use std::sync::Arc;

  use tokio::sync::Mutex;
  use tokio::task::LocalSet;

  use crate::observer::Observable;

  #[tokio::test]
  async fn test_get_clone() {
    let observable = Observable::new(20);
    observable.update_wait(25).await.unwrap();
    assert_eq!(25, observable.get_clone().await.unwrap());
  }

  #[tokio::test]
  async fn test_observer() {
    let observable = Observable::new(20);

    let observed = Arc::new(Mutex::new(0));
    let observed_clone = observed.clone();

    observable
      .observe(move |_old, new| {
        let observed_clone = observed_clone.clone();
        async move {
          let mut lock = observed_clone.lock().await;
          *lock = new;
        }
      })
      .await
      .unwrap();

    observable.update_wait(25).await.unwrap();

    assert_eq!(25, *(observed.lock().await));
  }

  #[tokio::test]
  async fn test_stop_observing() {
    let observable = Observable::new(20);

    let observed = Arc::new(Mutex::new(0));
    let observed_clone = observed.clone();

    let handle = observable
      .observe(move |_old, new| {
        let observed_clone = observed_clone.clone();
        async move {
          let mut lock = observed_clone.lock().await;
          *lock = new;
        }
      })
      .await
      .unwrap();

    observable.update_wait(25).await.unwrap();
    assert_eq!(25, *(observed.lock().await));
    handle.stop_observing().await.unwrap();
    observable.update_wait(30).await.unwrap();
    assert_eq!(25, *(observed.lock()).await);
  }

  #[tokio::test]
  async fn test_stop_observing_on_handle_drop() {
    let observable = Observable::new(20);

    let observed = Arc::new(Mutex::new(0));
    let observed_clone = observed.clone();

    let handle = observable
      .observe(move |_old, new| {
        let observed_clone = observed_clone.clone();
        async move {
          let mut lock = observed_clone.lock().await;
          *lock = new;
        }
      })
      .await
      .unwrap()
      .stop_on_drop();

    observable.update_wait(25).await.unwrap();
    assert_eq!(25, *(observed.lock().await));
    drop(handle);
    observable.update_wait(30).await.unwrap();
    assert_eq!(25, *(observed.lock()).await);
  }

  #[tokio::test]
  async fn test_local_observer() {
    struct NonSendStruct(i32);
    impl !Send for NonSendStruct {}
    impl !Sync for NonSendStruct {}

    let observable = Observable::new(20);

    let observed = Arc::new(Mutex::new(0));
    let observed_clone = observed.clone();
    let local_set = LocalSet::new();
    local_set.spawn_local(async move {
      let non_send_instance = NonSendStruct(42);

      observable
        .observe_local(move |_old, new| {
          let observed_clone = observed_clone.clone();
          async move {
            let mut lock = observed_clone.lock().await;
            let _works = non_send_instance.0;
            *lock = new;
          }
        })
        .await
        .unwrap();

      observable.update_wait(25).await.unwrap();
    });
    local_set.await;
    assert_eq!(25, *(observed.lock().await));
  }
}
