use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("user not found")]
    UserNotFound,

    #[error("invalid credentials")]
    InvalidCredentials,

    #[error("request failed: {0}")]
    Request(#[from] wasi_fetch::Error),

    #[error("error: {0}")]
    Other(String),
}
