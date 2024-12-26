use crate::{concurrency_limit::ConcurrencyLimit, error::Error};

use super::ExecuteSync;

pub(crate) struct CurrentContextExecutor {
    concurrency_limit: ConcurrencyLimit,
}

impl CurrentContextExecutor {
    pub(crate) fn new(max_concurrency: Option<usize>) -> Self {
        Self {
            concurrency_limit: ConcurrencyLimit::new(max_concurrency),
        }
    }
}

impl ExecuteSync for CurrentContextExecutor {
    async fn execute_sync<F, R>(&self, f: F) -> Result<R, Error>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        let _permit = self.concurrency_limit.acquire_permit().await;

        Ok(f())
        // permit implicitly drops
    }
}
