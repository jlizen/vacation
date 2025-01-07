use std::time::Duration;

use futures_util::future::join_all;
use vacation::{
    execute, global_strategy, init, ChanceOfBlocking, ExecutorStrategy, GlobalStrategy,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn execute_directly() {
    init()
        .max_concurrency(3)
        .execute_directly()
        .install()
        .unwrap();

    assert_eq!(
        global_strategy(),
        GlobalStrategy::Initialized(ExecutorStrategy::ExecuteDirectly)
    );

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
            tokio::task::spawn(async move {
                execute(closure, ChanceOfBlocking::High, "test.operation").await
            })
            .await
        };
        futures.push(future);
    }

    let res = join_all(futures).await;

    let elapsed_millis = start.elapsed().as_millis();
    assert!(elapsed_millis < 50, "futures did not run concurrently");
    assert!(elapsed_millis > 20, "futures exceeded max concurrency");
    assert!(!res.iter().any(|res| res.is_err()));
}
