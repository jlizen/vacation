#[cfg(feature = "tokio_block_in_place")]
#[tokio::test(flavor = "multi_thread")]
async fn block_in_place_strategy() {
    use compute_heavy_future_executor::{
        global_strategy, global_strategy_builder, spawn_compute_heavy_future, CurrentStrategy,
        ExecutorStrategy,
    };

    global_strategy_builder()
        .unwrap()
        .initialize_block_in_place()
        .unwrap();

    let future = async { 5 };

    let res = spawn_compute_heavy_future(future).await.unwrap();
    assert_eq!(res, 5);

    assert_eq!(
        global_strategy(),
        CurrentStrategy::Initialized(ExecutorStrategy::BlockInPlace)
    );
}
