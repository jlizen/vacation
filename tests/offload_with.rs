#[cfg(all(feature = "tokio", feature = "future"))]
mod test {
    use std::time::{Duration, Instant};

    use futures_util::future::join_all;

    #[tokio::test]
    async fn offload_with() {
        vacation::init()
            .max_concurrency(3)
            .spawn_blocking()
            .install()
            .unwrap();

        let start = Instant::now();

        let mut futures = Vec::new();

        for _ in 0..6 {
            let inner_future = Box::pin(async move {
                tokio::time::sleep(Duration::from_millis(20)).await;
                true
            });
            let future = vacation::future::builder()
                .future(inner_future)
                .offload_with(
                    vacation::future::OffloadWith::builder()
                        .get_offload_fn(|_inner_fut| {
                            Ok(Some(Box::new(Box::pin(vacation::execute(
                                || std::thread::sleep(Duration::from_millis(20)),
                                vacation::ChanceOfBlocking::High,
                            )))))
                        })
                        .incorporate_fn(|_, _| Ok(())),
                )
                .while_waiting_for_offload(vacation::future::WhileWaitingMode::PassThroughInnerPoll)
                .build();

            futures.push(future);
        }
        let res = join_all(futures).await;
        assert!(res.iter().all(|res| *res));

        let elapsed_millis = start.elapsed().as_millis();
        assert!(
            elapsed_millis < 60,
            "inner fut did not run concurrently with work, and/or futures did not run concurrently with each other"
        );
        assert!(elapsed_millis > 20, "futures exceeded max concurrency");
    }
}
