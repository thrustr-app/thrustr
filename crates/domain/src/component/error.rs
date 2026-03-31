use std::fmt;

#[derive(Debug, Clone)]
pub enum Error {
    Auth(String),
    Config(String),
    Other(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Config(msg) => write!(f, "Configuration error: {msg}"),
            Error::Auth(msg) => write!(f, "Authentication error: {msg}"),
            Error::Other(msg) => write!(f, "Error: {msg}"),
        }
    }
}

impl std::error::Error for Error {}
