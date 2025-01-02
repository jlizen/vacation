use tokio::runtime::Handle;

use crate::{concurrency_limit::ConcurrencyLimit, error::Error};

use super::Execute;

pub(crate) struct SpawnBlocking {
    concurrency_limit: ConcurrencyLimit,
    handle: Handle,
}

impl SpawnBlocking {
    pub(crate) fn new(handle: Handle, max_concurrency: Option<usize>) -> Self {
        let concurrency_limit = ConcurrencyLimit::new(max_concurrency);

        Self {
            concurrency_limit,
            handle,
        }
    }
}

impl Execute for SpawnBlocking {
    async fn execute<F, R>(&self, f: F) -> Result<R, Error>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        let _permit = self.concurrency_limit.acquire_permit().await;

        self.handle
            .spawn_blocking(f)
            .await
            .map_err(Error::JoinError)
        // permit implicitly drops
    }
}
