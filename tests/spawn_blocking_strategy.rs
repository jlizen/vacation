#[cfg(feature = "tokio")]
mod test {
    use std::time::Duration;

    use futures_util::future::join_all;

    use vacation::{execute, global_strategy, init, ExecutorStrategy, GlobalStrategy};

    #[tokio::test]
    async fn spawn_blocking() {
        init()
            .max_concurrency(3)
            .spawn_blocking()
            .install()
            .unwrap();

        assert_eq!(
            global_strategy(),
            GlobalStrategy::Initialized(ExecutorStrategy::SpawnBlocking)
        );

        let start = std::time::Instant::now();

        let mut futures = Vec::new();

        let closure = || {
            std::thread::sleep(Duration::from_millis(15));
            5
        };

        // note that we also are racing against concurrency from other tests in this module
        for _ in 0..6 {
            let future = async move {
                execute(
                    closure,
                    vacation::ExecuteContext::new(vacation::ChanceOfBlocking::Frequent),
                )
                .await
            };
            futures.push(future);
        }

        let res = join_all(futures).await;

        let elapsed_millis = start.elapsed().as_millis();
        assert!(elapsed_millis < 60, "futures did not run concurrently");
        assert!(elapsed_millis > 20, "futures exceeded max concurrency");
        assert!(!res.iter().any(|res| res.is_err()));
    }
}
