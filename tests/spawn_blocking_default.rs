#[cfg(feature = "tokio")]
#[tokio::test]
async fn spawn_blocking_strategy() {
    use std::time::Duration;

    use compute_heavy_future_executor::{
        execute_sync, global_sync_strategy, ExecutorStrategy, GlobalStrategy,
    };

    let closure = || {
        std::thread::sleep(Duration::from_millis(15));
        5
    };

    let res = execute_sync(closure).await.unwrap();
    assert_eq!(res, 5);

    assert_eq!(
        global_sync_strategy(),
        GlobalStrategy::Default(ExecutorStrategy::SpawnBlocking)
    );
}
