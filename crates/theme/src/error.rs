use thiserror::Error;

pub type Result<T> = std::result::Result<T, ThemeError>;

#[derive(Debug, Error)]
pub enum ThemeError {
    #[error("theme '{0}' not found")]
    NotFound(String),
}
