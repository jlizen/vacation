#[cfg(feature = "tokio")]
#[tokio::test]
async fn spawn_blocking_strategy() {
    use compute_heavy_future_executor::{
        execute_compute_heavy_future, global_strategy, CurrentStrategy, ExecutorStrategy,
    };

    let future = async { 5 };

    let res = execute_compute_heavy_future(future).await.unwrap();
    assert_eq!(res, 5);

    assert_eq!(
        global_strategy(),
        CurrentStrategy::Default(ExecutorStrategy::SpawnBlocking)
    );
}
