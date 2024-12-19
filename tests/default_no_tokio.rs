#[cfg(not(feature = "tokio"))]
#[tokio::test]
async fn default_to_current_context_non_tokio() {
    use compute_heavy_future_executor::spawn_compute_heavy_future;

    let future = async { 5 };

    let res = spawn_compute_heavy_future(future).await.unwrap();
    assert_eq!(res, 5);
}
