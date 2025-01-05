use crate::ExecutorStrategy;

/// An error from the custom executor
#[non_exhaustive]
#[derive(Debug)]
pub enum Error {
    /// Executor has already had a global strategy configured.
    AlreadyInitialized(ExecutorStrategy),
    /// Issue listening on the custom executor response channel.
    RecvError(tokio::sync::oneshot::error::RecvError),
    /// Error enforcing concurrency
    Semaphore(tokio::sync::AcquireError),
    /// Dynamic error from the custom executor closure
    BoxError(Box<dyn std::error::Error + Send + Sync>),
    #[cfg(feature = "tokio")]
    /// Background spawn blocking task panicked
    JoinError(tokio::task::JoinError),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::AlreadyInitialized(strategy) => write!(
                f,
                "global strategy is already initialized with strategy: {strategy:#?}"
            ),
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

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::BoxError(err) => Some(err.source()?),
            _ => None,
        }
    }
}
