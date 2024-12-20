use crate::{
    concurrency_limit::ConcurrencyLimit,
    error::{Error, InvalidConfig},
    ComputeHeavyFutureExecutor,
};

use tokio::runtime::{Handle, RuntimeFlavor};

pub(crate) struct BlockInPlaceExecutor {
    concurrency_limit: ConcurrencyLimit,
}

impl BlockInPlaceExecutor {
    pub(crate) fn new(max_concurrency: Option<usize>) -> Result<Self, Error> {
        log::info!("initializing compute-heavy executor with block in place strategy, max concurrency: {:#?}", max_concurrency);

        match Handle::current().runtime_flavor() {
            RuntimeFlavor::MultiThread => (),
            #[cfg(tokio_unstable)]
            RuntimeFlavor::MultiThreadAlt => (),
            flavor => Err(Error::InvalidConfig(InvalidConfig {
                field: "current tokio runtime",
                received: format!("{flavor:#?}"),
                allowed: "MultiThread",
            }))?,
        };

        Ok(Self {
            concurrency_limit: ConcurrencyLimit::new(max_concurrency),
        })
    }
}

impl ComputeHeavyFutureExecutor for BlockInPlaceExecutor {
    async fn execute<F, O>(&self, fut: F) -> Result<O, Error>
    where
        F: std::future::Future<Output = O> + Send + 'static,
        O: Send + 'static,
    {
        let _permit = self.concurrency_limit.acquire_permit().await;

        Ok(tokio::task::block_in_place(move || {
            tokio::runtime::Handle::current().block_on(async { fut.await })
        }))
        // permit implicitly drops
    }
}
