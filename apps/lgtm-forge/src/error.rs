//! Application errors for future review operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(String),
}

impl From<Error> for connectrpc::ConnectError {
    fn from(error: Error) -> Self {
        match error {
            Error::Invalid(message) => Self::invalid_argument(message),
        }
    }
}
