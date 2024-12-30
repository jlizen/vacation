use std::time::Duration;

use futures_util::future::join_all;
use vacation::{
    execute_sync, global_sync_strategy, global_sync_strategy_builder, ChanceOfBlocking,
    ExecutorStrategy, GlobalStrategy,
};

fn initialize() {
    // we are racing all tests against the single oncelock
    let _ = global_sync_strategy_builder()
        .max_concurrency(3)
        .initialize_current_context();
}

#[tokio::test]
async fn current_context_strategy() {
    initialize();

    let closure = || {
        std::thread::sleep(Duration::from_millis(15));
        5
    };

    let res = execute_sync(closure, ChanceOfBlocking::High).await.unwrap();
    assert_eq!(res, 5);

    assert_eq!(
        global_sync_strategy(),
        GlobalStrategy::Initialized(ExecutorStrategy::CurrentContext)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn current_context_concurrency() {
    initialize();

    let start = std::time::Instant::now();

    let mut futures = Vec::new();

    let closure = || {
        std::thread::sleep(Duration::from_millis(15));
        5
    };

    // note that we also are racing against concurrency from other tests in this module
    for _ in 0..6 {
        // we need to spawn tasks since otherwise we'll just block the current worker thread
        let future = async move {
            tokio::task::spawn(async move { execute_sync(closure, ChanceOfBlocking::High).await })
                .await
        };
        futures.push(future);
    }
    tokio::time::sleep(Duration::from_millis(5)).await;

    join_all(futures).await;

    let elapsed_millis = start.elapsed().as_millis();
    assert!(elapsed_millis < 50, "futures did not run concurrently");

    assert!(elapsed_millis > 20, "futures exceeded max concurrency");
}
