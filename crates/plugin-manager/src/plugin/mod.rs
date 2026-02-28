use crate::{Storefront, StorefrontPre};
use ports::managers::Plugin as PluginTrait;
use ports::metadata::{Image, Metadata};
use ports::providers::{StorefrontProvider, StorefrontProviderError, StorefrontProviderStatus};
use ports::storage::ExtensionStorage;
use semver::Version;
use std::sync::{Arc, Mutex};
use wasmtime::{Engine, Store};

mod manifest;
mod state;
mod storefront_provider;

pub use manifest::*;
pub use state::PluginState;

pub(crate) struct PluginBuilder {
    manifest: PluginManifest,
    engine: Engine,
    storage: Arc<dyn ExtensionStorage>,
    icon: Option<Image>,
    storefront_pre: Option<StorefrontPre<PluginState>>,
}

impl PluginBuilder {
    pub(crate) fn new(
        manifest: PluginManifest,
        engine: Engine,
        storage: Arc<dyn ExtensionStorage>,
    ) -> Self {
        Self {
            manifest,
            engine,
            storage,
            icon: None,
            storefront_pre: None,
        }
    }

    pub(crate) fn icon(mut self, icon: Option<Image>) -> Self {
        self.icon = icon;
        self
    }

    pub(crate) fn storefront_pre(
        mut self,
        storefront_pre: Option<StorefrontPre<PluginState>>,
    ) -> Self {
        self.storefront_pre = storefront_pre;
        self
    }

    pub(crate) fn build(self) -> Plugin {
        Plugin {
            manifest: self.manifest,
            engine: self.engine,
            storage: self.storage,
            icon: self.icon,
            storefront_pre: self.storefront_pre,
            storefront_provider_status: Mutex::new(StorefrontProviderStatus::Initializing),
        }
    }
}

pub struct Plugin {
    manifest: PluginManifest,
    icon: Option<Image>,
    engine: Engine,
    storage: Arc<dyn ExtensionStorage>,

    storefront_pre: Option<StorefrontPre<PluginState>>,
    storefront_provider_status: Mutex<StorefrontProviderStatus>,
}

impl Plugin {
    pub(crate) fn as_storefront_provider(self: &Arc<Self>) -> Option<Arc<dyn StorefrontProvider>> {
        self.storefront_pre
            .is_some()
            .then(|| Arc::clone(self) as Arc<dyn StorefrontProvider>)
    }

    pub(crate) async fn instantiate_storefront(
        &self,
    ) -> Result<(Storefront, Store<PluginState>), StorefrontProviderError> {
        let storefront_pre = self
            .storefront_pre
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

impl Metadata for Plugin {
    fn id(&self) -> &str {
        &self.manifest.plugin.id
    }

    fn name(&self) -> &str {
        &self.manifest.plugin.name
    }

    fn description(&self) -> Option<&str> {
        self.manifest.plugin.description.as_deref()
    }

    fn icon(&self) -> Option<&Image> {
        self.icon.as_ref()
    }
}

impl PluginTrait for Plugin {
    fn version(&self) -> &Version {
        &self.manifest.plugin.version
    }

    fn authors(&self) -> &[String] {
        &self.manifest.plugin.authors
    }
}
