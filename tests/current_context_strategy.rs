use compute_heavy_future_executor::{initialize_current_context_strategy, spawn_compute_heavy_future};

#[tokio::test]
async fn current_context_strategy() {
    initialize_current_context_strategy();

    let future = async { 5 };

    let res = spawn_compute_heavy_future(future).await.unwrap();
    assert_eq!(res, 5);
}