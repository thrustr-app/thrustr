use crate::exports::thrustr::plugin::base::Error as PluginError;
use crate::{StorefrontPlugin, StorefrontPluginPre};
use ports::capabilities::Storefront;
use ports::managers::Plugin as PluginTrait;
use ports::manifest::{Image, Manifest, Origin};
use ports::storage::ExtensionStorage;
use semver::Version;
use std::sync::{Arc, Mutex};
use wasmtime::{Engine, Store};

mod manifest;
mod state;
mod storefront;

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
            origin: Origin::Plugin(self.manifest.plugin.id.clone()),
            manifest: self.manifest,
            icon: self.icon,
            status: Mutex::new(PluginStatus::Inactive),
            engine: self.engine,
            storage: self.storage,
            storefront_pre: self.storefront_pre,
        }
    }
}

pub struct Plugin {
    manifest: PluginManifest,
    origin: Origin,
    icon: Option<Image>,
    status: Mutex<PluginStatus>,

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

    pub(crate) async fn init(&self) -> Result<(), PluginError> {
        {
            let mut lock = self.status.lock().unwrap();
            match *lock {
                PluginStatus::Inactive => *lock = PluginStatus::Initializing,
                _ => {
                    return Err(PluginError::Other(
                        "Plugin is already initializing or active".into(),
                    ));
                }
            }
        }

        event::emit("storefront");

        let result: Result<(), PluginError> = match self.instantiate_storefront().await {
            Ok((instance, mut store)) => instance
                .thrustr_plugin_base()
                .call_init(&mut store)
                .await
                .map_err(|e| PluginError::Other(format!("Wasm call failed: {e}")))?
                .map_err(Into::into),
            Err(e) => Err(e.into()),
        };

        *self.status.lock().unwrap() = match &result {
            Ok(_) => PluginStatus::Active,
            Err(e) => PluginStatus::Error(e.clone()),
        };

        event::emit("storefront");

        result
    }
}

impl Manifest for Plugin {
    fn id(&self) -> &str {
        &self.manifest.plugin.id
    }

    fn name(&self) -> &str {
        &self.manifest.plugin.name
    }

    fn origin(&self) -> &Origin {
        &self.origin
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
