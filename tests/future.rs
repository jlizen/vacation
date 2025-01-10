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
            let inner_fut = Box::pin(async move {
                // make sure we don't finish on first poll, and then this will be complete by the
                // time offloaded work finishes
                tokio::time::sleep(Duration::from_millis(2)).await;
                true
            });
            let future = vacation::future::builder()
                .future(inner_fut)
                .get_offload(|inner_fut| {
                    // we just always return work in this case, we get more granular with unit tests
                    // in reality you probably would want to call a method on the future or check a buffer
                    // or something
                    Ok(vacation::future::OffloadWork::HasWork(Box::new(
                        async move {
                            match vacation::execute(
                                || std::thread::sleep(Duration::from_millis(20)),
                                vacation::ExecuteContext {
                                    chance_of_blocking: vacation::ChanceOfBlocking::High,
                                    namespace: "test.operation",
                                },
                            )
                            .await
                            {
                                Ok(_) => Ok(inner_fut),
                                Err(_) => Err(false),
                            }
                        },
                    )))
                })
                .build();

            futures.push(future);
        }
        let res = join_all(futures).await;
        assert!(res.iter().all(|res| *res));

        let elapsed_millis = start.elapsed().as_millis();
        assert!(
            elapsed_millis < 60,
            "futures did not run concurrently with each other"
        );
        assert!(elapsed_millis > 30, "futures exceeded max concurrency");

        // now let's test the case where the inner yields, and then vacation fails, and bubbles up an error
        // this demonstrates that we don't poll the inner again while outer is polling, since it would
        // otherwise finish first and succeed
        let future = vacation::future::builder()
            .future(Box::pin(async {
                tokio::time::sleep(Duration::from_millis(20)).await;
                true
            }))
            .get_offload(|_inner_fut| {
                // we just always return work in this case, we get more granular with unit tests
                // in reality you probably would want to call a method on the future or check a buffer
                // or something
                Ok(vacation::future::OffloadWork::HasWork(Box::new(
                    async move {
                        match vacation::execute(
                            || std::thread::sleep(Duration::from_millis(50)),
                            vacation::ExecuteContext {
                                chance_of_blocking: vacation::ChanceOfBlocking::High,
                                namespace: "test.operation",
                            },
                        )
                        .await
                        {
                            // we still return an error, so the offloaded work always fails
                            Ok(_) => Err(false),
                            Err(_) => Err(false),
                        }
                    },
                )))
            })
            .build();

        let res = future.await;
        // if the inner future completed, we would have success, but instead we delay polling and return
        // an error after the slower vacation work finishes
        assert_eq!(res, false);

        // and a case where there is no work to be done
        let future = vacation::future::builder()
            .future(Box::pin(async {
                tokio::time::sleep(Duration::from_millis(20)).await;
                true
            }))
            .get_offload(|inner_fut| {
                // we just never work in this case, we get more granular with unit tests
                // in reality you probably would want to call a method on the future or check a buffer
                // or something
                Ok(vacation::future::OffloadWork::NoWork(inner_fut))
            })
            .build();

        let res = future.await;
        assert_eq!(res, true);
    }
}
