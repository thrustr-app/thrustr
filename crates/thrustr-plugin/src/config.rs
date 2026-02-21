use crate::{StorefrontProviderError, wit::thrustr::plugin::config::get};

pub use crate::wit::thrustr::plugin::config::Error as ConfigError;

pub struct Config;

impl Config {
    pub fn get(field_id: &str) -> Result<String, ConfigError> {
        Ok(get(field_id)?)
    }
}

impl From<ConfigError> for StorefrontProviderError {
    fn from(err: ConfigError) -> StorefrontProviderError {
        match err {
            ConfigError::Internal(msg) => StorefrontProviderError::Other(msg),
        }
    }
}
