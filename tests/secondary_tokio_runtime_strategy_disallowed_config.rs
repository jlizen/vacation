#[cfg(feature = "tokio")]
#[tokio::test]
#[should_panic]
async fn secondary_tokio_runtime_strategy_disallowed_config() {
    use compute_heavy_future_executor::{
        initialize_secondary_tokio_runtime_strategy_and_config, spawn_compute_heavy_future,
    };

    initialize_secondary_tokio_runtime_strategy_and_config(Some(30), Some(50));

    let future = async { 5 };

    let res = spawn_compute_heavy_future(future).await.unwrap();
    assert_eq!(res, 5);
}
