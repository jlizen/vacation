#[cfg(feature = "tokio")]
#[tokio::test]
async fn secondary_tokio_runtime_strategy() {
    use compute_heavy_future_executor::{initialize_secondary_tokio_runtime_strategy, spawn_compute_heavy_future};

    initialize_secondary_tokio_runtime_strategy();

    let future = async { 5 };
    
    let res = spawn_compute_heavy_future(future).await.unwrap();
    assert_eq!(res, 5);
}