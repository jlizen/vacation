use core::fmt;

#[non_exhaustive]
#[derive(Debug)]
pub enum Error {
    RecvError(tokio::sync::oneshot::error::RecvError),
    #[cfg(feature = "tokio")]
    JoinError(tokio::task::JoinError),
    BoxError(Box<dyn std::error::Error + Send + Sync>),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(feature = "tokio")]
            Error::JoinError(err) => write!(f, "error joining tokio handle in spawn_blocking executor: {err}"),
            Error::BoxError(err) => write!(f, "custom executor error: {err}"),
            Error::RecvError(err) => write!(f, "error in custom executor response channel: {err}"),
        }
    }
}
