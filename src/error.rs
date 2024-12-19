use core::fmt;

#[non_exhaustive]
#[derive(Debug)]
pub enum Error {
    CustomExecutorOutputTypeMismatch,
    #[cfg(feature = "tokio")]
    JoinError(String),
    BoxError(Box<dyn std::error::Error + Send + Sync>),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::CustomExecutorOutputTypeMismatch => write!(f, "custom executor returned a different output type than the input future's output type"),
            #[cfg(feature = "tokio")]
            Error::JoinError(err) => write!(f, "error joining tokio handle: {err}"),
            Error::BoxError(err) => write!(f, "{err}"),
        }
    }
}
