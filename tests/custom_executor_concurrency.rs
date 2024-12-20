use std::time::Duration;

use compute_heavy_future_executor::{
    global_strategy_builder, spawn_compute_heavy_future, CustomExecutorClosure,
};
use futures_util::future::join_all;
#[tokio::test]
async fn custom_strategy_concurrency() {
    let closure: CustomExecutorClosure = Box::new(|fut| {
        Box::new(async move {
            let res = fut.await;
            Ok(res)
        })
    });

    global_strategy_builder()
        .unwrap()
        .max_concurrency(3)
        .initialize_custom_executor(closure)
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
