use std::{sync::OnceLock, time::Duration};

use futures_util::future::join_all;
use rayon::ThreadPool;
use vacation::{
    execute, init, ChanceOfBlocking, CustomClosureInput, CustomClosureOutput, ExecutorStrategy,
    GlobalStrategy,
};

static THREADPOOL: OnceLock<ThreadPool> = OnceLock::new();

#[tokio::test]
async fn custom_concurrency() {
    THREADPOOL.get_or_init(|| rayon::ThreadPoolBuilder::default().build().unwrap());

    let custom_closure = |input: CustomClosureInput| {
        Box::new(async move {
            THREADPOOL.get().unwrap().spawn(input.work);
            Ok(())
        }) as CustomClosureOutput
    };

    init()
        .max_concurrency(3)
        .custom_executor(custom_closure)
        .install()
        .unwrap();

    assert_eq!(
        vacation::global_strategy(),
        GlobalStrategy::Initialized(ExecutorStrategy::Custom)
    );

    let start = std::time::Instant::now();

    let mut futures = Vec::new();

    let closure = || {
        std::thread::sleep(Duration::from_millis(15));
        5
    };

    // note that we also are racing against concurrency from other tests in this module
    for _ in 0..6 {
        futures.push(execute(closure, ChanceOfBlocking::High, "test.operation"));
    }

    let res = join_all(futures).await;

    let elapsed_millis = start.elapsed().as_millis();
    assert!(elapsed_millis < 50, "futures did not run concurrently");
    assert!(elapsed_millis > 20, "futures exceeded max concurrency");
    assert!(!res.iter().any(|res| res.is_err()));
}
