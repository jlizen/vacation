use crate::{concurrency_limit::ConcurrencyLimit, error::Error};

use super::Execute;

pub(crate) struct ExecuteDirectly {
    concurrency_limit: ConcurrencyLimit,
}

impl ExecuteDirectly {
    pub(crate) fn new(max_concurrency: Option<usize>) -> Self {
        Self {
            concurrency_limit: ConcurrencyLimit::new(max_concurrency),
        }
    }
}

impl Execute for ExecuteDirectly {
    async fn execute<F, R>(&self, f: F) -> Result<R, Error>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        let _permit = self.concurrency_limit.acquire_permit().await;

        Ok(f())
        // permit implicitly drops
    }
}
