#[cfg(feature = "secondary_tokio_runtime")]
#[tokio::test]
async fn secondary_tokio_runtime_strategy() {
    use compute_heavy_future_executor::{global_strategy, spawn_compute_heavy_future};

    global_strategy()
        .unwrap()
        .initialize_secondary_tokio_runtime()
        .unwrap();

    let future = async { 5 };

    let res = spawn_compute_heavy_future(future).await.unwrap();
    assert_eq!(res, 5);
}
