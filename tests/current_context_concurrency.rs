use std::time::Duration;

use compute_heavy_future_executor::{global_strategy_builder, spawn_compute_heavy_future};
use futures_util::future::join_all;

#[tokio::test]
async fn current_context_concurrency() {
    global_strategy_builder()
        .unwrap()
        .max_concurrency(3)
        .initialize_current_context()
        .unwrap();

    let start = std::time::Instant::now();

    let mut futures = Vec::new();

    for _ in 0..5 {
        // can't use std::thread::sleep because this is all in the same thread
        let future = async move { tokio::time::sleep(Duration::from_millis(15)).await };
        futures.push(spawn_compute_heavy_future(future));
    }

    join_all(futures).await;

    let elapsed_millis = start.elapsed().as_millis();
    assert!(elapsed_millis < 50, "futures did not run concurrently");

    assert!(elapsed_millis > 20, "futures exceeded max concurrency");
}
