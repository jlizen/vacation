use std::{future::Future, pin::Pin};

use tokio::select;

use crate::{error::Error, ComputeHeavyFutureExecutor};

/// A closure that accepts an arbitrary future and polls it to completion
/// via its preferred strategy.
pub type CustomExecutorClosure = Box<
    dyn Fn(
            Pin<Box<dyn Future<Output = ()> + Send + 'static>>,
        )
            -> Box<dyn Future<Output = Result<(), Box<dyn std::error::Error + Send + Sync>>>>
        + Send
        + Sync,
>;

pub(crate) struct CustomExecutor {
    pub(crate) closure: CustomExecutorClosure,
}

impl ComputeHeavyFutureExecutor for CustomExecutor {
    async fn execute<F, O>(&self, fut: F) -> Result<O, Error>
    where
        F: Future<Output = O> + Send + 'static,
        O: Send + 'static,
    {
        let (mut tx, rx) = tokio::sync::oneshot::channel();
        let wrapped_future = Box::pin(async move {
            select! {
                _ = tx.closed() => {
                    // receiver already dropped, don't need to do anything
                    // cancel the background future
                },
                result = fut => {
                    // if this fails, the receiver already dropped, so we don't need to do anything
                    let _ = tx.send(result);
                }
            }
        });

        // if our custom executor future resolves to an error, we know it will never send
        // the response so we immediately return
        if let Err(err) = Box::into_pin((self.closure)(wrapped_future)).await {
            return Err(Error::BoxError(err));
        }

        rx.await.map_err(|err| Error::RecvError(err))
    }
}
