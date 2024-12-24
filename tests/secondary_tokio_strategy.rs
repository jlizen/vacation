#[cfg(feature = "secondary_tokio_runtime")]
mod test {
    use std::time::Duration;

    use futures_util::future::join_all;
    use tokio::select;

    use compute_heavy_future_executor::{
        execute_compute_heavy_future, global_strategy, global_strategy_builder, CurrentStrategy,
        ExecutorStrategy,
    };

    fn initialize() {
        // we are racing all tests against the single oncelock
        let _ = global_strategy_builder()
            .max_concurrency(3)
            .initialize_secondary_tokio_runtime();
    }

    #[tokio::test]
    async fn secondary_tokio_runtime_strategy() {
        initialize();

        let future = async { 5 };

        let res = execute_compute_heavy_future(future).await.unwrap();
        assert_eq!(res, 5);

        assert_eq!(
            global_strategy(),
            CurrentStrategy::Initialized(ExecutorStrategy::SecondaryTokioRuntime)
        );
    }

    #[tokio::test]
    async fn secondary_tokio_runtime_concurrency() {
        initialize();

        let start = std::time::Instant::now();

        let mut futures = Vec::new();

        for _ in 0..5 {
            let future = async move { std::thread::sleep(Duration::from_millis(15)) };
            futures.push(execute_compute_heavy_future(future));
        }

        join_all(futures).await;

        let elapsed_millis = start.elapsed().as_millis();
        assert!(elapsed_millis < 50, "futures did not run concurrently");

        assert!(elapsed_millis > 20, "futures exceeded max concurrency");
    }

    #[tokio::test]
    async fn secondary_tokio_runtime_strategy_cancel_safe() {
        initialize();

        let (tx, mut rx) = tokio::sync::oneshot::channel::<()>();
        let future = async move {
            {
                tokio::time::sleep(Duration::from_secs(60)).await;
                let _ = tx.send(());
            }
        };

        select! {
            _ = tokio::time::sleep(Duration::from_millis(4)) => { },
            _ = execute_compute_heavy_future(future) => {}
        }

        tokio::time::sleep(Duration::from_millis(8)).await;

        // future should have been cancelled when spawn compute heavy future was dropped
        assert_eq!(
            rx.try_recv(),
            Err(tokio::sync::oneshot::error::TryRecvError::Closed)
        );
    }
}
