#[cfg(feature = "secondary_tokio_runtime")]
#[tokio::test]
async fn secondary_tokio_runtime_builder_allowed_config() {
    use compute_heavy_future_executor::{execute_compute_heavy_future, global_strategy_builder};

    global_strategy_builder()
        .max_concurrency(5)
        .secondary_tokio_runtime_builder()
        .channel_size(10)
        .niceness(5)
        .thread_count(2)
        .initialize()
        .unwrap();

    let future = async { 5 };

    let res = execute_compute_heavy_future(future).await.unwrap();
    assert_eq!(res, 5);
}
