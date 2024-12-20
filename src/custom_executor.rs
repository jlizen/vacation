use std::{future::Future, pin::Pin};

use crate::{
    concurrency_limit::ConcurrencyLimit, error::Error, make_future_cancellable,
    ComputeHeavyFutureExecutor,
};

/// A closure that accepts an arbitrary future and polls it to completion
/// via its preferred strategy.
pub type CustomExecutorClosure = Box<
    dyn Fn(
            Pin<Box<dyn Future<Output = ()> + Send + 'static>>,
        ) -> Box<
            dyn Future<Output = Result<(), Box<dyn std::error::Error + Send + Sync>>>
                + Send
                + 'static,
        > + Send
        + Sync,
>;

pub(crate) struct CustomExecutor {
    closure: CustomExecutorClosure,
    concurrency_limit: ConcurrencyLimit,
}

impl CustomExecutor {
    pub(crate) fn new(closure: CustomExecutorClosure, max_concurrency: Option<usize>) -> Self {
        log::info!(
            "initializing compute-heavy executor with custom strategy, max concurrency: {:#?}",
            max_concurrency
        );

        Self {
            closure,
            concurrency_limit: ConcurrencyLimit::new(max_concurrency),
        }
    }
}

impl ComputeHeavyFutureExecutor for CustomExecutor {
    async fn execute<F, O>(&self, fut: F) -> Result<O, Error>
    where
        F: Future<Output = O> + Send + 'static,
        O: Send + 'static,
    {
        let _permit = self.concurrency_limit.acquire_permit().await;

        let (wrapped_future, rx) = make_future_cancellable(fut);

        // if our custom executor future resolves to an error, we know it will never send
        // the response so we immediately return
        if let Err(err) = Box::into_pin((self.closure)(Box::pin(wrapped_future))).await {
            return Err(Error::BoxError(err));
        }

        rx.await.map_err(|err| Error::RecvError(err))

        // permit implicitly drops
    }
}
