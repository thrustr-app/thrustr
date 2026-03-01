use crate::exports::thrustr::plugin::base::Error as PluginError;
use crate::{StorefrontProviderPlugin, StorefrontProviderPluginPre};
use ports::managers::Plugin as PluginTrait;
use ports::metadata::{Image, Metadata};
use ports::providers::StorefrontProvider;
use ports::storage::ExtensionStorage;
use semver::Version;
use std::sync::{Arc, Mutex};
use wasmtime::{Engine, Store};

mod manifest;
mod state;
mod storefront_provider;

pub use manifest::*;
pub use state::PluginState;

#[derive(Clone, Debug)]
enum PluginStatus {
    Inactive,
    Initializing,
    Active,
    Error(PluginError),
}

pub(crate) struct PluginBuilder {
    manifest: PluginManifest,
    engine: Engine,
    storage: Arc<dyn ExtensionStorage>,
    icon: Option<Image>,
    storefront_pre: Option<StorefrontProviderPluginPre<PluginState>>,
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
        storefront_pre: Option<StorefrontProviderPluginPre<PluginState>>,
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
            storefront_provider_pre: self.storefront_pre,
            status: Mutex::new(PluginStatus::Inactive),
        }
    }
}

pub struct Plugin {
    manifest: PluginManifest,
    icon: Option<Image>,
    engine: Engine,
    storage: Arc<dyn ExtensionStorage>,
    status: Mutex<PluginStatus>,

    storefront_provider_pre: Option<StorefrontProviderPluginPre<PluginState>>,
}

impl Plugin {
    pub(crate) fn as_storefront_provider(self: &Arc<Self>) -> Option<Arc<dyn StorefrontProvider>> {
        self.storefront_provider_pre
            .is_some()
            .then(|| Arc::clone(self) as Arc<dyn StorefrontProvider>)
    }

    pub(crate) async fn instantiate_storefront_provider(
        &self,
    ) -> Result<(StorefrontProviderPlugin, Store<PluginState>), PluginError> {
        let storefront_pre = self
            .storefront_provider_pre
            .as_ref()
            .ok_or(PluginError::Other("Not a storefront".into()))?;

        let mut store = Store::new(
            &self.engine,
            PluginState::new(self.id(), self.storage.clone()),
        );

        let instance = storefront_pre
            .instantiate_async(&mut store)
            .await
            .map_err(|e| PluginError::Other(format!("Instantiation failed: {e}")))?;

        Ok((instance, store))
    }

    pub(crate) async fn init(&self) -> Result<(), PluginError> {
        *self.status.lock().unwrap() = PluginStatus::Initializing;

        event::emit("storefront");

        let (instance, mut store) = self.instantiate_storefront_provider().await?;

        let result: Result<_, PluginError> = instance
            .thrustr_plugin_base()
            .call_init(&mut store)
            .await
            .map_err(|e| PluginError::Other(format!("Wasm call failed: {e}")))?
            .map_err(Into::into);

        *self.status.lock().unwrap() = result
            .as_ref()
            .map(|_| PluginStatus::Active)
            .unwrap_or_else(|e| PluginStatus::Error(e.clone()));

        event::emit("storefront");

        result
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
