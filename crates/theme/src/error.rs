use std::{error::Error, fmt::Display};

#[derive(Debug)]
pub enum ThemeError {
    NotFound(String),
}

impl Display for ThemeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ThemeError::NotFound(name) => write!(f, "Theme '{}' not found", name),
        }
    }
}

impl Error for ThemeError {}
