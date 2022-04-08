## async-observer
---
A simple, asynchronous implementation of the observer pattern in Rust.
```rust
let observable = Observable::new(42);

observable.observe(|old, new| async move {
  println!("Value changed from {} to {}", old, new);
}).await.unwrap();

observable.update(1337).unwrap();
assert_eq!(1337, observable.get_clone().await.unwrap());
```

The crate provides a generic `Observable` struct that can wrap cloneable values. You can then register asynchronous callbacks that will receive clones of the old and new value when the observable is being updated. The Observable is implemented using tokio tasks and channels and may therefore not be suitable for some use-cases.

### Features
- Wait for observers when updating the value
  ```rust
  observable.update_wait(69).await.unwrap();
  ```
- Stop observing 
    ```rust
    let handle = observable.observe(|old, new| async move {
      // Do observing...
    }).await.unwrap();
    // Do stuff
    handle.stop_observing().await.unwrap();
    ```
- Stop observing when the handle drops
  ```rust
  let handle = observable.observe(|old, new| async move {
    // Do observing...
  }).await.unwrap().stop_on_drop();
  // Do stuff
  drop(handle); // -> not observing anymore
  ```
- Use Tokio's `LocalSet` to be able to use non-send Futures
  ```rust
  struct NonSendStruct(i32);
  impl !Send for NonSendStruct {}
  impl !Sync for NonSendStruct {}
  
  let local_set = LocalSet::new();
  local_set.spawn_local(async move {
    let non_send_instance = NonSendStruct(1337);
    let observable = Observable::new(42);
    observable.observe_local(|old, new| async move {
       // Do observing and use moved, non-send instance
       let yay = non_send_instance.0;
    }).await.unwrap();    
  });
  local_set.await;
  ```
- Observe non-send values in a `LocalSet`
  ```rust
  let local_set = LocalSet::new();
  local_set.spawn_local(async move {  
    let observable = Observable::new_local(NonSendStruct(42));
    observable.observe_local(|old, new| async move {
       // Do observing
       let yay = new.0;
    }).await.unwrap();    
  });
  local_set.await;
  ```
  
### Alternatives

Some alternatives that implement the Observer pattern differently or enable similar architectures:

- [Pharos](https://crates.io/crates/pharos) - a more powerful Observable/Event-dispatcher
- [async-observable](https://crates.io/crates/async-observable) - a similar, but tokio-independent Observable implementation
- [tokios broadcast channels](https://docs.rs/tokio/latest/tokio/sync/broadcast/index.html) - can be used to implement simple event dispatching