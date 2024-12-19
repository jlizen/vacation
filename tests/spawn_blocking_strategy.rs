use compute_heavy_future_executor::{initialize_spawn_blocking_strategy, spawn_compute_heavy_future};


#[cfg(feature = "tokio")]
#[tokio::test]
async fn spawn_blocking_strategy() {
    initialize_spawn_blocking_strategy();

    let future = async { 5 };

    let res = spawn_compute_heavy_future(future).await.unwrap();
    assert_eq!(res, 5);

}