use crate::exports::thrustr::plugin::base::Error as PluginError;
use crate::{StorefrontPlugin, StorefrontPluginPre};
use async_trait::async_trait;
use ports::capabilities::{
    Component, ComponentError, ComponentOrigin, ComponentStatus, Image, Storefront,
};
use ports::storage::ComponentStorage;
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
    storage: Arc<dyn ComponentStorage>,
    icon: Option<Image>,
    storefront_pre: Option<StorefrontPluginPre<PluginState>>,
}

impl PluginBuilder {
    pub(crate) fn new(
        manifest: PluginManifest,
        engine: Engine,
        storage: Arc<dyn ComponentStorage>,
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
            origin: ComponentOrigin::Plugin(self.manifest.plugin.id.clone()),
            manifest: self.manifest,
            icon: self.icon,
            status: Mutex::new(ComponentStatus::Inactive),
            engine: self.engine,
            storage: self.storage,
            storefront_pre: self.storefront_pre,
        }
    }
}

pub struct Plugin {
    manifest: PluginManifest,
    origin: ComponentOrigin,
    icon: Option<Image>,
    status: Mutex<ComponentStatus>,

    engine: Engine,
    storage: Arc<dyn ComponentStorage>,

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

    fn set_status(&self, status: ComponentStatus) {
        *self.status.lock().unwrap() = status;
        event::emit("capability");
    }
}

#[async_trait]
impl Component for Plugin {
    fn id(&self) -> &str {
        &self.manifest.plugin.id
    }

    fn name(&self) -> &str {
        &self.manifest.plugin.name
    }

    fn origin(&self) -> &ComponentOrigin {
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

    fn status(&self) -> ComponentStatus {
        self.status.lock().unwrap().clone()
    }

    async fn init(&self) -> Result<(), ComponentError> {
        {
            let mut lock = self.status.lock().unwrap();
            match *lock {
                ComponentStatus::Inactive => *lock = ComponentStatus::Initializing,
                _ => {
                    return Err(ComponentError::Initialization(
                        "Plugin is already initializing or active".into(),
                    ));
                }
            }
        }

        self.set_status(ComponentStatus::Initializing);

        let result: Result<(), ComponentError> = match self.instantiate_storefront().await {
            Ok((instance, mut store)) => instance
                .thrustr_plugin_base()
                .call_init(&mut store)
                .await
                .map_err(|e| ComponentError::Initialization(format!("Wasm call failed: {e}")))?
                .map_err(|e: PluginError| ComponentError::Initialization(format!("{e:?}"))),
            Err(e) => Err(ComponentError::Initialization(format!("{e:?}"))),
        };

        self.set_status(match &result {
            Ok(_) => ComponentStatus::Active,
            Err(e) => ComponentStatus::Error(e.clone()),
        });

        result
    }
}
