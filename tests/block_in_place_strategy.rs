#[cfg(feature = "tokio")]
#[tokio::test(flavor = "multi_thread")]
async fn block_in_place_strategy() {
    use compute_heavy_future_executor::{global_strategy, spawn_compute_heavy_future};

    global_strategy()
        .unwrap()
        .initialize_block_in_place()
        .unwrap();

    let future = async { 5 };

    let res = spawn_compute_heavy_future(future).await.unwrap();
    assert_eq!(res, 5);
}
