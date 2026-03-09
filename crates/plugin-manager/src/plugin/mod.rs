use crate::exports::thrustr::plugin::base::{AuthFlow as PluginAuthFlow, Error as PluginError};
use crate::{StorefrontPlugin, StorefrontPluginPre};
use async_trait::async_trait;
use ports::capabilities::{Capability, Storefront};
use ports::component::{
    AuthFlow, Component, Config, Error as ComponentError, Image, Metadata, Origin, Status,
};
use ports::storage::ComponentStorage;
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
        let metadata = Metadata {
            id: self.manifest.plugin.id.clone(),
            name: self.manifest.plugin.name,
            origin: Origin::Plugin(self.manifest.plugin.id),
            description: self.manifest.plugin.description,
            icon: self.icon,
            version: self.manifest.plugin.version,
            authors: self.manifest.plugin.authors,
        };

        Plugin {
            metadata,
            config: self.manifest.config,
            status: Mutex::new(Status::Inactive),
            engine: self.engine,
            storage: self.storage,
            storefront_pre: self.storefront_pre,
        }
    }
}

pub struct Plugin {
    metadata: Metadata,
    config: Option<Config>,
    status: Mutex<Status>,

    engine: Engine,
    storage: Arc<dyn ComponentStorage>,

    storefront_pre: Option<StorefrontPluginPre<PluginState>>,
}

impl Plugin {
    pub(crate) async fn instantiate_storefront(
        &self,
    ) -> Result<(StorefrontPlugin, Store<PluginState>), PluginError> {
        let storefront_pre = self
            .storefront_pre
            .as_ref()
            .ok_or(PluginError::Other("Not a storefront".into()))?;

        let mut store = Store::new(
            &self.engine,
            PluginState::new(&self.metadata.id, self.storage.clone()),
        );

        let instance = storefront_pre
            .instantiate_async(&mut store)
            .await
            .map_err(|e| PluginError::Other(format!("Instantiation failed: {e}")))?;

        Ok((instance, store))
    }

    fn set_status(&self, status: Status) {
        *self.status.lock().unwrap() = status;
        event::emit("component");
    }
}

#[async_trait]
impl Component for Plugin {
    fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }

    fn config(&self) -> Option<&Config> {
        self.config.as_ref()
    }

    fn storefront(self: Arc<Self>) -> Option<Arc<dyn Storefront>> {
        self.storefront_pre
            .is_some()
            .then(|| self as Arc<dyn Storefront>)
    }

    async fn init(&self) -> Result<(), ComponentError> {
        {
            let mut lock = self.status.lock().unwrap();
            match *lock {
                Status::Inactive => *lock = Status::Initializing,
                _ => {
                    return Err(ComponentError::Other(
                        "Plugin is already initializing or active".into(),
                    ));
                }
            }
        }

        let result = match self.instantiate_storefront().await {
            Ok((instance, mut store)) => instance
                .thrustr_plugin_base()
                .call_init(&mut store)
                .await
                .map_err(|e| ComponentError::Other(format!("Wasm call failed: {e}")))?
                .map_err(Into::into),
            Err(e) => Err(ComponentError::Other(format!("{e:?}"))),
        };

        self.set_status(match &result {
            Ok(_) => Status::Active,
            Err(e) => Status::Error(e.clone()),
        });

        result
    }

    async fn get_login_flow(&self) -> Result<Option<AuthFlow>, ComponentError> {
        let result = match self.instantiate_storefront().await {
            Ok((instance, mut store)) => instance
                .thrustr_plugin_base()
                .call_get_login_flow(&mut store)
                .await
                .map_err(|e| ComponentError::Other(format!("Wasm call failed: {e}")))?
                .map_err(Into::into),
            Err(e) => Err(ComponentError::Other(format!("{e:?}"))),
        };

        if let Err(e) = &result {
            self.set_status(Status::Error(e.clone()));
        }

        result.map(|o| o.map(Into::into))
    }

    async fn get_logout_flow(&self) -> Result<Option<AuthFlow>, ComponentError> {
        let result = match self.instantiate_storefront().await {
            Ok((instance, mut store)) => instance
                .thrustr_plugin_base()
                .call_get_logout_flow(&mut store)
                .await
                .map_err(|e| ComponentError::Other(format!("Wasm call failed: {e}")))?
                .map_err(Into::into),
            Err(e) => Err(ComponentError::Other(format!("{e:?}"))),
        };

        self.set_status(match &result {
            Ok(_) => Status::Active,
            Err(e) => Status::Error(e.clone()),
        });

        result.map(|o| o.map(Into::into))
    }

    async fn login(&self, url: String, body: String) -> Result<(), ComponentError> {
        let result = match self.instantiate_storefront().await {
            Ok((instance, mut store)) => instance
                .thrustr_plugin_base()
                .call_login(&mut store, &url, &body)
                .await
                .map_err(|e| ComponentError::Other(format!("Wasm call failed: {e}")))?
                .map_err(Into::into),
            Err(e) => Err(ComponentError::Other(format!("{e:?}"))),
        };

        self.set_status(match &result {
            Ok(_) => Status::Active,
            Err(e) => Status::Error(e.clone()),
        });

        result
    }

    async fn logout(&self, url: String, body: String) -> Result<(), ComponentError> {
        let result = match self.instantiate_storefront().await {
            Ok((instance, mut store)) => instance
                .thrustr_plugin_base()
                .call_logout(&mut store, &url, &body)
                .await
                .map_err(|e| ComponentError::Other(format!("Wasm call failed: {e}")))?
                .map_err(Into::into),
            Err(e) => Err(ComponentError::Other(format!("{e:?}"))),
        };

        self.set_status(match &result {
            Ok(_) => Status::Active,
            Err(e) => Status::Error(e.clone()),
        });

        result
    }

    async fn validate_config(&self, fields: &[(String, String)]) -> Result<(), ComponentError> {
        let result = match self.instantiate_storefront().await {
            Ok((instance, mut store)) => instance
                .thrustr_plugin_base()
                .call_validate_config(&mut store, fields)
                .await
                .map_err(|e| ComponentError::Other(format!("Wasm call failed: {e}")))?
                .map_err(Into::into),
            Err(e) => Err(ComponentError::Other(format!("{e:?}"))),
        };

        if let Err(e) = &result {
            self.set_status(Status::Error(e.clone()));
        }

        result
    }
}

impl Capability for Plugin {
    fn component(&self) -> &dyn Component {
        self as &dyn Component
    }
}

impl From<PluginAuthFlow> for AuthFlow {
    fn from(value: PluginAuthFlow) -> Self {
        AuthFlow {
            url: value.url,
            target: value.target,
        }
    }
}

impl From<PluginError> for ComponentError {
    fn from(value: PluginError) -> Self {
        match value {
            PluginError::NotAutorized(msg) => ComponentError::Authentication(msg),
            PluginError::Configuration(msg) => ComponentError::Configuration(msg),
            PluginError::Other(msg) => ComponentError::Other(msg),
        }
    }
}
