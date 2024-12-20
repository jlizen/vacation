use compute_heavy_future_executor::{global_strategy, spawn_compute_heavy_future};

#[tokio::test]
async fn current_context_strategy() {
    global_strategy()
        .unwrap()
        .initialize_current_context()
        .unwrap();

    let future = async { 5 };

    let res = spawn_compute_heavy_future(future).await.unwrap();
    assert_eq!(res, 5);
}
