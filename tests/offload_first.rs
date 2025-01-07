#[cfg(all(feature = "tokio", feature = "future"))]
mod test {
    use std::time::{Duration, Instant};

    use futures_util::future::join_all;

    #[tokio::test]
    async fn offload_first() {
        vacation::init()
            .max_concurrency(3)
            .spawn_blocking()
            .install()
            .unwrap();

        let start = Instant::now();

        let mut futures = Vec::new();

        for _ in 0..6 {
            let future = vacation::future::builder()
                .offload_first(
                    vacation::future::OffloadFirst::builder()
                        .offload_future(vacation::execute(
                            || {
                                std::thread::sleep(Duration::from_millis(10));
                                true
                            },
                            vacation::ChanceOfBlocking::High,
                            "test.operation",
                        ))
                        .incorporate_fn(|res| {
                            Ok(Box::pin(async move {
                                tokio::time::sleep(Duration::from_millis(10)).await;
                                res
                            }))
                        }),
                )
                .build();

            futures.push(future);
        }
        let res = join_all(futures).await;
        assert!(res.into_iter().all(|res| res.is_ok() && res.unwrap()));

        let elapsed_millis = start.elapsed().as_millis();
        assert!(
            elapsed_millis < 60,
            "futures did not run concurrently with each other"
        );
        assert!(elapsed_millis > 20, "futures exceeded max concurrency");
    }
}
