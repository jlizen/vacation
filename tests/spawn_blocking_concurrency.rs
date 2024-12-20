#[cfg(feature = "tokio")]
#[tokio::test]
async fn spawn_blocking_concurrency() {
    use std::time::Duration;

    use compute_heavy_future_executor::{global_strategy_builder, spawn_compute_heavy_future};
    use futures_util::future::join_all;

    global_strategy_builder()
        .unwrap()
        .max_concurrency(3)
        .initialize_spawn_blocking()
        .unwrap();

    let start = std::time::Instant::now();

    let mut futures = Vec::new();

    for _ in 0..5 {
        let future = async move { std::thread::sleep(Duration::from_millis(15)) };
        futures.push(spawn_compute_heavy_future(future));
    }

    join_all(futures).await;

    let elapsed_millis = start.elapsed().as_millis();
    assert!(elapsed_millis < 60, "futures did not run concurrently");

    assert!(elapsed_millis > 20, "futures exceeded max concurrency");
}
