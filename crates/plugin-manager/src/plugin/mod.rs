use crate::{Storefront, StorefrontPre};
use ports::providers::{StorefrontProvider, StorefrontProviderError};
use ports::storage::ExtensionStorage;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use wasmtime::{Engine, Store};

mod state;
mod storefront;

pub use state::PluginState;

#[derive(Serialize, Deserialize, Debug)]
pub struct PluginManifest {
    pub plugin: PluginInfo,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub authors: Vec<String>,
    pub version: Version,
    pub description: Option<String>,
}

pub struct Plugin {
    manifest: PluginManifest,
    engine: Engine,
    storage: Arc<dyn ExtensionStorage>,

    storefront: Option<StorefrontPre<PluginState>>,
}

impl Plugin {
    pub(crate) fn new(
        manifest: PluginManifest,
        engine: Engine,
        storage: Arc<dyn ExtensionStorage>,
    ) -> Self {
        Self {
            manifest,
            engine,
            storage,
            storefront: None,
        }
    }

    pub fn id(&self) -> &str {
        &self.manifest.plugin.id
    }

    pub fn set_storefront(&mut self, storefront: Option<StorefrontPre<PluginState>>) {
        self.storefront = storefront;
    }

    pub fn as_storefront(self: &Arc<Self>) -> Option<Arc<dyn StorefrontProvider>> {
        self.storefront
            .is_some()
            .then(|| Arc::clone(self) as Arc<dyn StorefrontProvider>)
    }

    pub(crate) async fn instantiate_storefront(
        &self,
    ) -> Result<(Storefront, Store<PluginState>), StorefrontProviderError> {
        let storefront_pre = self
            .storefront
            .as_ref()
            .ok_or(StorefrontProviderError::Other("Not a storefront".into()))?;

        let mut store = Store::new(
            &self.engine,
            PluginState::new(self.id(), self.storage.clone()),
        );

        let instance = storefront_pre
            .instantiate_async(&mut store)
            .await
            .map_err(|e| StorefrontProviderError::Other(format!("Instantiation failed: {e}")))?;

        Ok((instance, store))
    }
}
