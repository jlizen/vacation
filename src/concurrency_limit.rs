use std::sync::Arc;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::error::Error;

/// Wrapper around semaphore that turns it into a non-op if no limit is provided
/// or semaphore channel is closed
pub(crate) struct ConcurrencyLimit {
    semaphore: Option<Arc<Semaphore>>,
}

impl ConcurrencyLimit {
    /// Accepts none in case no concurrency
    pub(crate) fn new(limit: Option<usize>) -> Self {
        let semaphore = limit.map(|limit| Arc::new(Semaphore::new(limit)));

        Self { semaphore }
    }

    /// Waits on a permit to the semaphore if configured, otherwise immediately returns.
    ///
    /// Internally turns errors into a no-op (`None`) and outputs log lines.
    pub(crate) async fn acquire_permit(&self) -> Option<OwnedSemaphorePermit> {
        match self.semaphore.clone() {
            Some(semaphore) => {
                match semaphore
                    .acquire_owned()
                    .await
                    .map_err(|err| Error::Semaphore(err))
                {
                    Ok(permit) => Some(permit),
                    Err(err) => {
                        log::error!("failed to acquire permit: {err}");
                        None
                    }
                }
            }
            None => None,
        }
    }
}
