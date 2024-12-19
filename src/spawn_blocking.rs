use crate::{error::Error, ComputeHeavyFutureExecutor};

pub(crate) struct SpawnBlockingExecutor {}

impl ComputeHeavyFutureExecutor for SpawnBlockingExecutor {
    async fn execute<F, O>(&self, fut: F) -> Result<O, Error>
    where
        F: std::future::Future<Output = O> + Send + 'static,
        O: Send + 'static,
    {
        tokio::task::spawn_blocking(move || {
            tokio::runtime::Handle::current().block_on(async { fut.await })
        })
        .await
        .map_err(|err| Error::JoinError(err))
    }
}
