#[cfg(feature = "tokio_block_in_place")]
mod test {
    use std::time::Duration;

    use compute_heavy_future_executor::{
        execute_compute_heavy_future, global_strategy, global_strategy_builder, CurrentStrategy,
        ExecutorStrategy,
    };
    use futures_util::future::join_all;

    fn initialize() {
        // we are racing all tests against the single oncelock
        let _ = global_strategy_builder()
            .max_concurrency(3)
            .initialize_block_in_place();
    }

    #[cfg(feature = "tokio_block_in_place")]
    #[tokio::test(flavor = "multi_thread")]
    async fn block_in_place_strategy() {
        initialize();

        let future = async { 5 };

        let res = execute_compute_heavy_future(future).await.unwrap();
        assert_eq!(res, 5);

        assert_eq!(
            global_strategy(),
            CurrentStrategy::Initialized(ExecutorStrategy::BlockInPlace)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 10)]
    async fn block_in_place_concurrency() {
        initialize();

        let start = std::time::Instant::now();

        let mut futures = Vec::new();

        for _ in 0..5 {
            let future = async move { std::thread::sleep(Duration::from_millis(15)) };
            // we need to spawn here since otherwise block in place will cancel other futures inside the same task,
            // ref https://docs.rs/tokio/latest/tokio/task/fn.block_in_place.html
            let future =
                tokio::task::spawn(async move { execute_compute_heavy_future(future).await });
            futures.push(future);
        }

        join_all(futures).await;

        let elapsed_millis = start.elapsed().as_millis();
        assert!(elapsed_millis < 50, "futures did not run concurrently");

        assert!(elapsed_millis > 20, "futures exceeded max concurrency");
    }
}
