use crate::{error::Error, ComputeHeavyFutureExecutor};

pub(crate) struct CurrentContextExecutor {}

impl ComputeHeavyFutureExecutor for CurrentContextExecutor {
    async fn execute<F, O>(&self, fut: F) -> Result<O, Error>
    where
        F: std::future::Future<Output = O> + Send + 'static,
        O: Send + 'static,
    {
        Ok(fut.await)
    }
}
