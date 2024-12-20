#[cfg(feature = "tokio")]
#[tokio::test]
async fn spawn_blocking_strategy_cancellable() {
    use std::time::Duration;

    use compute_heavy_future_executor::{
        initialize_spawn_blocking_strategy, spawn_compute_heavy_future,
    };
    use tokio::select;

    initialize_spawn_blocking_strategy();

    let (tx, mut rx) = tokio::sync::oneshot::channel::<()>();
    let future = async move { {
        tokio::time::sleep(Duration::from_secs(60)).await;
        let _ = tx.send(());
    } };

    select! {
        _ = tokio::time::sleep(Duration::from_millis(10)) => { },
        _ = spawn_compute_heavy_future(future) => {}
    }

    tokio::time::sleep(Duration::from_millis(10)).await;

    // future should have been cancelled when spawn compute heavy future was dropped
    assert_eq!(rx.try_recv(), Err(tokio::sync::oneshot::error::TryRecvError::Closed));
}
