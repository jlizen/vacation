use crate::{
    concurrency_limit::ConcurrencyLimit, error::Error, make_future_cancellable,
    ComputeHeavyFutureExecutor,
};

pub(crate) struct SpawnBlockingExecutor {
    concurrency_limit: ConcurrencyLimit,
}

impl SpawnBlockingExecutor {
    pub(crate) fn new(max_concurrency: Option<usize>) -> Self {
        let concurrency_limit = ConcurrencyLimit::new(max_concurrency);

        Self { concurrency_limit }
    }
}

impl ComputeHeavyFutureExecutor for SpawnBlockingExecutor {
    async fn execute<F, O>(&self, fut: F) -> Result<O, Error>
    where
        F: std::future::Future<Output = O> + Send + 'static,
        O: Send + 'static,
    {
        let _permit = self.concurrency_limit.acquire_permit().await;

        let (wrapped_future, rx) = make_future_cancellable(fut);

        if let Err(err) = tokio::task::spawn_blocking(move || {
            tokio::runtime::Handle::current().block_on(wrapped_future)
        })
        .await
        {
            return Err(Error::JoinError(err));
        }

        rx.await.map_err(|err| Error::RecvError(err))
    }
}
