use std::{sync::OnceLock, time::Duration};

use futures_util::future::join_all;
use rayon::ThreadPool;
use vacation::{execute, init, ChanceOfBlocking, CustomClosureInput, CustomClosureOutput};

static THREADPOOL: OnceLock<ThreadPool> = OnceLock::new();

fn initialize() {
    THREADPOOL.get_or_init(|| rayon::ThreadPoolBuilder::default().build().unwrap());

    let custom_closure = |f: CustomClosureInput| {
        Box::new(async move {
            THREADPOOL.get().unwrap().spawn(f);
            Ok(())
        }) as CustomClosureOutput
    };

    let _ = init()
        .max_concurrency(3)
        .custom_executor(custom_closure)
        .install();
}

#[tokio::test]
async fn custom_strategy() {
    initialize();

    let closure = || {
        std::thread::sleep(Duration::from_millis(15));
        5
    };

    let res = execute(closure, ChanceOfBlocking::High).await.unwrap();
    assert_eq!(res, 5);
}

#[tokio::test]
async fn custom_concurrency() {
    initialize();

    let start = std::time::Instant::now();

    let mut futures = Vec::new();

    let closure = || {
        std::thread::sleep(Duration::from_millis(15));
        5
    };
    tokio::time::sleep(Duration::from_millis(5)).await;

    // note that we also are racing against concurrency from other tests in this module
    for _ in 0..6 {
        futures.push(execute(closure, ChanceOfBlocking::High));
    }

    join_all(futures).await;

    let elapsed_millis = start.elapsed().as_millis();
    assert!(elapsed_millis < 50, "futures did not run concurrently");

    assert!(elapsed_millis > 20, "futures exceeded max concurrency");
}
