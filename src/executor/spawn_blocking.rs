use tokio::runtime::Handle;

use crate::{concurrency_limit::ConcurrencyLimit, error::Error};

use super::ExecuteSync;

pub(crate) struct SpawnBlockingExecutor {
    concurrency_limit: ConcurrencyLimit,
    handle: Handle,
}

impl SpawnBlockingExecutor {
    pub(crate) fn new(max_concurrency: Option<usize>) -> Self {
        let concurrency_limit = ConcurrencyLimit::new(max_concurrency);

        Self {
            concurrency_limit,
            handle: Handle::current(),
        }
    }
}

impl ExecuteSync for SpawnBlockingExecutor {
    async fn execute_sync<F, R>(&self, f: F) -> Result<R, Error>
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
