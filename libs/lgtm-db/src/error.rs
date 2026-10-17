#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Sql(#[from] sea_orm::DbErr),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn invalid(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}
