use tokio::select;

use crate::{error::Error, ComputeHeavyFutureExecutor};

pub(crate) struct SpawnBlockingExecutor {}

impl ComputeHeavyFutureExecutor for SpawnBlockingExecutor {
    async fn execute<F, O>(&self, fut: F) -> Result<O, Error>
    where
        F: std::future::Future<Output = O> + Send + 'static,
        O: Send + 'static,
    {
        let (mut tx, rx) = tokio::sync::oneshot::channel();

        let wrapped_future = async {
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
        };

        if let Err(err) = tokio::task::spawn_blocking(move || {
            tokio::runtime::Handle::current().block_on(wrapped_future)
        })
        .await
        {
            return Err(Error::JoinError(err));
        }

        rx.await.map_err(|err| Error::RecvError(err))
    }
}
