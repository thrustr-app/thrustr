use thiserror::Error;

pub(crate) type Result<T> = std::result::Result<T, ThemeError>;

#[derive(Debug, Error)]
pub enum ThemeError {
    #[error("Theme '{0}' not found")]
    NotFound(String),
}
