use crate::{error::Error, ComputeHeavyFutureExecutor};

pub(crate) struct BlockInPlaceExecutor {}

impl ComputeHeavyFutureExecutor for BlockInPlaceExecutor {
    async fn execute<F, O>(&self, fut: F) -> Result<O, Error>
    where
        F: std::future::Future<Output = O> + Send + 'static,
        O: Send + 'static,
    {
        Ok(tokio::task::block_in_place(move || {
            tokio::runtime::Handle::current().block_on(async { fut.await })
        }))
    }
}
