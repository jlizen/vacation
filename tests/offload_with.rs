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

        let future = vacation::future::builder()
            .future(Box::pin(async { 
                tokio::time::sleep(Duration::from_millis(20)).await;
                Ok::<bool, &'static str>(true) 
            }))
            .offload_with(
                vacation::future::OffloadWith::builder()
                    .get_offload_fn(|_| {
                        Ok(Some(Box::new(Box::pin(vacation::execute(
                            || std::thread::sleep(Duration::from_millis(50)),
                            vacation::ChanceOfBlocking::High,
                        )))))
                    })
                    .incorporate_fn(|_, _: Result<(), vacation::Error>| Err(Err("foo"))),
            )
            .while_waiting_for_offload(vacation::future::WhileWaitingMode::SuppressInnerPoll)
            .build::<()>();

        let res = future.await;
        // if the inner future completed, we would have success, but instead we delay polling and return
        // an error after the slower vacation work finishes

        // gets work on first poll,
        // executes work on second poll + gets more work
        // executes work on third poll, then finishes inner future and returns before getting more work
        assert_eq!(res, Err("foo"));
    }
}
