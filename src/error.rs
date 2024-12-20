use core::fmt;

use crate::ExecutorStrategy;

#[non_exhaustive]
#[derive(Debug)]
pub enum Error {
    AlreadyInitialized(ExecutorStrategy),
    InvalidConfig(InvalidConfig),
    RecvError(tokio::sync::oneshot::error::RecvError),
    Semaphore(tokio::sync::AcquireError),
    BoxError(Box<dyn std::error::Error + Send + Sync>),
    #[cfg(feature = "tokio")]
    JoinError(tokio::task::JoinError),
}

#[derive(Debug)]
pub struct InvalidConfig {
    pub field: &'static str,
    pub received: String,
    pub allowed: &'static str,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::AlreadyInitialized(strategy) => write!(
                f,
                "global strategy is already initialzed with strategy: {strategy:#?}"
            ),
            Error::InvalidConfig(err) => write!(f, "invalid config: {err:#?}"),
            Error::BoxError(err) => write!(f, "custom executor error: {err}"),
            Error::RecvError(err) => write!(f, "error in custom executor response channel: {err}"),
            Error::Semaphore(err) => write!(
                f,
                "concurrency limiter semaphore channel is closed, continuing: {err}"
            ),
            #[cfg(feature = "tokio")]
            Error::JoinError(err) => write!(
                f,
                "error joining tokio handle in spawn_blocking executor: {err}"
            ),
        }
    }
}
