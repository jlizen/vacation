use std::future::Future;

use crate::{concurrency_limit::ConcurrencyLimit, error::Error};

use super::Execute;

/// The input for the custom closure
pub type CustomClosureInput = Box<dyn FnOnce() + Send + 'static>;
/// The output type for the custom closure
pub type CustomClosureOutput =
    Box<dyn Future<Output = Result<(), Box<dyn std::error::Error + Send + Sync>>> + Send + 'static>;

/// A closure that accepts an arbitrary sync function and returns a future that executes it.
/// The Custom Executor will implicitly wrap the input function in a oneshot
/// channel to erase its input/output type.
pub(crate) type CustomClosure =
    Box<dyn Fn(CustomClosureInput) -> CustomClosureOutput + Send + Sync>;

pub(crate) struct Custom {
    closure: CustomClosure,
    concurrency_limit: ConcurrencyLimit,
}

impl Custom {
    pub(crate) fn new(closure: CustomClosure, max_concurrency: Option<usize>) -> Self {
        Self {
            closure,
            concurrency_limit: ConcurrencyLimit::new(max_concurrency),
        }
    }
}

impl Execute for Custom {
    // the compiler correctly is pointing out that the custom closure isn't guaranteed to call f.
    // but, we leave that to the implementer to guarantee since we are limited by working with static signatures
    #[allow(unused_variables)]
    async fn execute<F, R>(&self, f: F) -> Result<R, Error>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        let _permit = self.concurrency_limit.acquire_permit().await;

        let (tx, rx) = tokio::sync::oneshot::channel();

        let wrapped_input_closure = Box::new(move || {
            let _permit = _permit;
            let res = f();
            if tx.send(res).is_err() {
                log::trace!("custom sync executor foreground dropped before it could receive the result of the sync closure");
            }
            // permit implicitly drops after work finishes
        });

        Box::into_pin((self.closure)(wrapped_input_closure))
            .await
            .map_err(Error::BoxError)?;

        rx.await.map_err(Error::RecvError)
    }
}
