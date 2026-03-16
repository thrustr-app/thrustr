use thiserror::Error;
use wstd::http;

#[derive(Debug, Error)]
pub enum Error {
    #[error("user not found")]
    UserNotFound,

    #[error("invalid credentials")]
    InvalidCredentials,

    #[error("request failed: {0}")]
    Request(#[from] http::Error),

    #[error("error: {0}")]
    Other(String),
}
