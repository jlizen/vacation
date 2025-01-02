#[cfg(feature = "tokio")]
mod test {
    use std::time::Duration;

    use futures_util::future::join_all;

    use vacation::{
        execute, global_strategy, init, ChanceOfBlocking, ExecutorStrategy, GlobalStrategy,
    };

    fn initialize() {
        // we are racing all tests against the single oncelock
        let _ = init().max_concurrency(3).spawn_blocking().install();
    }

    #[tokio::test]
    async fn spawn_blocking_strategy() {
        initialize();

        let closure = || {
            std::thread::sleep(Duration::from_millis(15));
            5
        };

        let res = execute(closure, ChanceOfBlocking::High).await.unwrap();
        assert_eq!(res, 5);

        assert_eq!(
            global_strategy(),
            GlobalStrategy::Initialized(ExecutorStrategy::SpawnBlocking)
        );
    }

    #[tokio::test]
    async fn spawn_blocking_concurrency() {
        initialize();
        let start = std::time::Instant::now();

        let mut futures = Vec::new();

        let closure = || {
            std::thread::sleep(Duration::from_millis(15));
            5
        };

        // note that we also are racing against concurrency from other tests in this module
        for _ in 0..6 {
            let future = async move { execute(closure, ChanceOfBlocking::High).await };
            futures.push(future);
        }
        tokio::time::sleep(Duration::from_millis(5)).await;

        let res = join_all(futures).await;
        println!("{res:#?}");

        let elapsed_millis = start.elapsed().as_millis();
        assert!(elapsed_millis < 60, "futures did not run concurrently");

        assert!(elapsed_millis > 20, "futures exceeded max concurrency");
    }
}
