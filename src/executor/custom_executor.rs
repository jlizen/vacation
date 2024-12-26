use std::future::Future;

use crate::{concurrency_limit::ConcurrencyLimit, error::Error};

use super::ExecuteSync;

/// A closure that accepts an arbitrary sync function and returns a future that executes it.
/// The Custom Executor will implicitly wrap the input function in a oneshot
/// channel to erase its input/output type.
pub type CustomExecutorSyncClosure = Box<
    dyn Fn(
            Box<dyn FnOnce() + Send + 'static>,
        ) -> Box<
            dyn Future<Output = Result<(), Box<dyn std::error::Error + Send + Sync>>>
                + Send
                + 'static,
        > + Send
        + Sync,
>;

pub(crate) struct CustomExecutor {
    closure: CustomExecutorSyncClosure,
    concurrency_limit: ConcurrencyLimit,
}

impl CustomExecutor {
    pub(crate) fn new(closure: CustomExecutorSyncClosure, max_concurrency: Option<usize>) -> Self {
        Self {
            closure,
            concurrency_limit: ConcurrencyLimit::new(max_concurrency),
        }
    }
}

impl ExecuteSync for CustomExecutor {
    // the compiler correctly is pointing out that the custom closure isn't guaranteed to call f.
    // but, we leave that to the implementer to guarantee since we are limited by working with static signatures
    #[allow(unused_variables)]
    async fn execute_sync<F, R>(&self, f: F) -> Result<R, Error>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        let _permit = self.concurrency_limit.acquire_permit().await;

        let (tx, rx) = tokio::sync::oneshot::channel();

        let wrapped_input_closure = Box::new(|| {
            let res = f();
            if tx.send(res).is_err() {
                log::trace!("custom sync executor foreground dropped before it could receive the result of the sync closure");
            }
        });

        Box::into_pin((self.closure)(wrapped_input_closure))
            .await
            .map_err(Error::BoxError)?;

        rx.await.map_err(Error::RecvError)
        // permit implicitly drops
    }
}
