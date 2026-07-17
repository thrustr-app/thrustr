use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Error {
    #[error("Authentication error: {0}")]
    Auth(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Error: {0}")]
    Other(String),
}
