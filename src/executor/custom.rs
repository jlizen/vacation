use std::future::Future;

use crate::{concurrency_limit::ConcurrencyLimit, error::Error, ExecuteContext};

/// The input for the custom closure
pub struct CustomClosureInput {
    /// the actual work to execute, your custom closure must run this
    pub work: Box<dyn FnOnce() + Send + 'static>,
    /// caller-specified metadata that allows fine tuning strategies
    pub context: ExecuteContext,
}

impl std::fmt::Debug for CustomClosureInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CustomClosureInput")
            .field("context", &self.context)
            .finish()
    }
}

/// The output type for the custom closure
pub type CustomClosureOutput =
    Box<dyn Future<Output = Result<(), Box<dyn std::error::Error + Send + Sync>>> + Send + 'static>;

/// A closure that accepts an arbitrary sync function as well as an operation namespace
/// and returns a future that executes it.
///
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

    // the compiler correctly is pointing out that the custom closure isn't guaranteed to call f.
    // but, we leave that to the implementer to guarantee since we are limited by working with static signatures
    #[allow(unused_variables)]
    pub(crate) async fn execute<F, R>(&self, f: F, context: ExecuteContext) -> Result<R, Error>
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

        let input = CustomClosureInput {
            work: wrapped_input_closure,
            context,
        };

        Box::into_pin((self.closure)(input))
            .await
            .map_err(Error::BoxError)?;

        rx.await.map_err(Error::RecvError)
    }
}
