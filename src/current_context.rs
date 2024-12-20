use crate::{concurrency_limit::ConcurrencyLimit, error::Error, ComputeHeavyFutureExecutor};

pub(crate) struct CurrentContextExecutor {
    concurrency_limit: ConcurrencyLimit,
}

impl CurrentContextExecutor {
    pub(crate) fn new(max_concurrency: Option<usize>) -> Self {
        log::info!("initializing compute-heavy executor with current context strategy, max concurrency: {:#?}", max_concurrency);

        Self {
            concurrency_limit: ConcurrencyLimit::new(max_concurrency),
        }
    }
}

impl ComputeHeavyFutureExecutor for CurrentContextExecutor {
    async fn execute<F, O>(&self, fut: F) -> Result<O, Error>
    where
        F: std::future::Future<Output = O> + Send + 'static,
        O: Send + 'static,
    {
        let _permit = self.concurrency_limit.acquire_permit().await;

        Ok(fut.await)

        // implicit permit drop
    }
}
