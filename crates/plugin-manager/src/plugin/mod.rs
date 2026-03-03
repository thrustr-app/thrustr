use crate::exports::thrustr::plugin::base::Error as PluginError;
use crate::{StorefrontPlugin, StorefrontPluginPre};
use async_trait::async_trait;
use ports::capabilities::{
    CapabilityProvider, CapabilityProviderError, CapabilityProviderOrigin,
    CapabilityProviderStatus, Image, Storefront,
};
use ports::storage::ExtensionStorage;
use semver::Version;
use std::sync::{Arc, Mutex};
use wasmtime::{Engine, Store};

mod manifest;
mod state;
mod storefront;

pub use manifest::*;
pub use state::PluginState;

pub(crate) struct PluginBuilder {
    manifest: PluginManifest,
    engine: Engine,
    storage: Arc<dyn ExtensionStorage>,
    icon: Option<Image>,
    storefront_pre: Option<StorefrontPluginPre<PluginState>>,
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
        storefront_pre: Option<StorefrontPluginPre<PluginState>>,
    ) -> Self {
        self.storefront_pre = storefront_pre;
        self
    }

    pub(crate) fn build(self) -> Plugin {
        Plugin {
            origin: CapabilityProviderOrigin::Plugin(self.manifest.plugin.id.clone()),
            manifest: self.manifest,
            icon: self.icon,
            status: Mutex::new(CapabilityProviderStatus::Inactive),
            engine: self.engine,
            storage: self.storage,
            storefront_pre: self.storefront_pre,
        }
    }
}

pub struct Plugin {
    manifest: PluginManifest,
    origin: CapabilityProviderOrigin,
    icon: Option<Image>,
    status: Mutex<CapabilityProviderStatus>,

    engine: Engine,
    storage: Arc<dyn ExtensionStorage>,

    storefront_pre: Option<StorefrontPluginPre<PluginState>>,
}

impl Plugin {
    pub(crate) fn as_storefront(self: &Arc<Self>) -> Option<Arc<dyn Storefront>> {
        self.storefront_pre
            .is_some()
            .then(|| Arc::clone(self) as Arc<dyn Storefront>)
    }

    pub(crate) async fn instantiate_storefront(
        &self,
    ) -> Result<(StorefrontPlugin, Store<PluginState>), PluginError> {
        let storefront_pre = self
            .storefront_pre
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

    fn set_status(&self, status: CapabilityProviderStatus) {
        *self.status.lock().unwrap() = status;
        event::emit("capability");
    }
}

#[async_trait]
impl CapabilityProvider for Plugin {
    fn id(&self) -> &str {
        &self.manifest.plugin.id
    }

    fn name(&self) -> &str {
        &self.manifest.plugin.name
    }

    fn origin(&self) -> &CapabilityProviderOrigin {
        &self.origin
    }

    fn description(&self) -> Option<&str> {
        self.manifest.plugin.description.as_deref()
    }

    fn icon(&self) -> Option<&Image> {
        self.icon.as_ref()
    }

    fn version(&self) -> &Version {
        &self.manifest.plugin.version
    }

    fn authors(&self) -> &[String] {
        &self.manifest.plugin.authors
    }

    fn status(&self) -> CapabilityProviderStatus {
        self.status.lock().unwrap().clone()
    }

    async fn init(&self) -> Result<(), CapabilityProviderError> {
        {
            let mut lock = self.status.lock().unwrap();
            match *lock {
                CapabilityProviderStatus::Inactive => {
                    *lock = CapabilityProviderStatus::Initializing
                }
                _ => {
                    return Err(CapabilityProviderError::Initialization(
                        "Plugin is already initializing or active".into(),
                    ));
                }
            }
        }

        self.set_status(CapabilityProviderStatus::Initializing);

        let result: Result<(), CapabilityProviderError> = match self.instantiate_storefront().await
        {
            Ok((instance, mut store)) => instance
                .thrustr_plugin_base()
                .call_init(&mut store)
                .await
                .map_err(|e| {
                    CapabilityProviderError::Initialization(format!("Wasm call failed: {e}"))
                })?
                .map_err(|e: PluginError| {
                    CapabilityProviderError::Initialization(format!("{e:?}"))
                }),
            Err(e) => Err(CapabilityProviderError::Initialization(format!("{e:?}"))),
        };

        self.set_status(match &result {
            Ok(_) => CapabilityProviderStatus::Active,
            Err(e) => CapabilityProviderStatus::Error(e.clone()),
        });

        result
    }
}
