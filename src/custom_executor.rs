use std::{any::Any, future::Future, pin::Pin};

use crate::{error::Error, ComputeHeavyFutureExecutor};

/// The input future for a custom executor closure.
/// This is the regular future, with its output type erased to Any.
pub type AnyWrappedFuture =
    Pin<Box<dyn Future<Output = Box<dyn Any + Send + 'static>> + Send + 'static>>;

/// A closure that accepts a type-erased input future and returns a future that resolves to
/// either Ok(<type erased input future's output>) or Err(<boxed error representing executor errors>)
pub type CustomExecutorClosure = Box<
    dyn Fn(
            AnyWrappedFuture,
        ) -> Box<
            dyn Future<
                Output = Result<
                    Box<dyn Any + Send + 'static>,
                    Box<dyn std::error::Error + Send + Sync>,
                >,
            >,
        > + Send
        + Sync,
>;

// used for checking that the closure doesn't mutate the output type while working with Any
struct PrivateType {}

pub(crate) struct CustomExecutor {
    pub(crate) closure: CustomExecutorClosure,
}

impl CustomExecutor {
    pub(crate) async fn new(closure: CustomExecutorClosure) -> Self {
        let executor = Self { closure };

        let test_future = async { PrivateType {} };

        if let Err(_) = executor.execute(test_future).await {
            panic!(
                "CustomExecutor strategy initialized with a closure that \
            changes the future's output type"
            );
        }

        executor
    }
}

impl ComputeHeavyFutureExecutor for CustomExecutor {
    async fn execute<F, O>(&self, fut: F) -> Result<O, Error>
    where
        F: Future<Output = O> + Send + 'static,
        O: Send + 'static,
    {
        let wrapped_future = Box::pin(async move {
            let res = fut.await;
            Box::new(res) as Box<dyn Any + Send>
        });

        let executor_result = Box::into_pin((self.closure)(wrapped_future)).await;

        match executor_result {
            Ok(future_result) => {
                if let Ok(value) = future_result.downcast::<O>() {
                    Ok(*value)
                } else {
                    // we would love to include the output that we actually received, but we don't want
                    // to force display/debug bounds onto the future output
                    Err(Error::CustomExecutorOutputTypeMismatch)
                }
            }
            Err(err) => Err(Error::BoxError(err)),
        }
    }
}
