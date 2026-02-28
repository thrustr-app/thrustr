use crate::{
    exports::thrustr::plugin::base::Error as PluginError,
    plugin::{Plugin, PluginStatus},
};
use async_trait::async_trait;
use ports::providers::{StorefrontProvider, StorefrontProviderError, StorefrontProviderStatus};

#[async_trait]
impl StorefrontProvider for Plugin {
    fn status(&self) -> StorefrontProviderStatus {
        self.status.lock().unwrap().clone().into()
    }

    async fn init(&self) -> Result<(), StorefrontProviderError> {
        self.init().await.map_err(StorefrontProviderError::from)
    }
}

impl From<PluginError> for StorefrontProviderError {
    fn from(e: PluginError) -> Self {
        match e {
            PluginError::NotAutorized(msg) => StorefrontProviderError::NotAutorized(msg),
            PluginError::Other(msg) => StorefrontProviderError::Other(msg),
        }
    }
}

impl From<PluginStatus> for StorefrontProviderStatus {
    fn from(status: PluginStatus) -> Self {
        match status {
            PluginStatus::Inactive => StorefrontProviderStatus::Inactive,
            PluginStatus::Initializing => StorefrontProviderStatus::Initializing,
            PluginStatus::Active => StorefrontProviderStatus::Active,
            PluginStatus::Error(e) => {
                StorefrontProviderStatus::Error(StorefrontProviderError::from(e))
            }
        }
    }
}
