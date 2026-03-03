use crate::{
    exports::thrustr::plugin::base::Error as PluginError,
    plugin::{Plugin, PluginStatus},
};
use async_trait::async_trait;
use ports::capabilities::{Storefront, StorefrontError, StorefrontStatus};

#[async_trait]
impl Storefront for Plugin {
    fn status(&self) -> StorefrontStatus {
        self.status.lock().unwrap().clone().into()
    }

    async fn init(&self) -> Result<(), StorefrontError> {
        self.init().await.map_err(StorefrontError::from)
    }
}

impl From<PluginError> for StorefrontError {
    fn from(e: PluginError) -> Self {
        match e {
            PluginError::NotAutorized(msg) => StorefrontError::NotAutorized(msg),
            PluginError::Other(msg) => StorefrontError::Other(msg),
        }
    }
}

impl From<PluginStatus> for StorefrontStatus {
    fn from(status: PluginStatus) -> Self {
        match status {
            PluginStatus::Inactive => StorefrontStatus::Inactive,
            PluginStatus::Initializing => StorefrontStatus::Initializing,
            PluginStatus::Active => StorefrontStatus::Active,
            PluginStatus::Error(e) => StorefrontStatus::Error(StorefrontError::from(e)),
        }
    }
}
