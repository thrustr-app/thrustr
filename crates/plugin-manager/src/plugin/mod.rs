use crate::exports::thrustr::plugin::base::{AuthFlow as PluginAuthFlow, Error as PluginError};
use crate::{StorefrontPlugin, StorefrontPluginPre};
use async_trait::async_trait;
use domain::capabilities::{Capability, Storefront};
use domain::component::{
    AuthFlow, Component, Config, Error as ComponentError, Image, LoginForm, LoginMethod, Metadata,
    Origin, Status,
};
use domain::storage::ComponentStorage;
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
            login_form: self.manifest.auth,
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
    login_form: Option<LoginForm>,
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
}

#[async_trait]
impl Component for Plugin {
    fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }

    fn set_status(&self, status: Status) {
        *self.status.lock().unwrap() = status;
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
        let (instance, mut store) = self
            .instantiate_storefront()
            .await
            .map_err(|e| ComponentError::Other(format!("{e:?}")))?;

        instance
            .thrustr_plugin_base()
            .call_init(&mut store)
            .await
            .map_err(|e| ComponentError::Other(format!("Wasm call failed: {e}")))?
            .map_err(ComponentError::from)
    }

    async fn get_login_method(&self) -> Result<Option<LoginMethod>, ComponentError> {
        let (instance, mut store) = self
            .instantiate_storefront()
            .await
            .map_err(|e| ComponentError::Other(format!("{e:?}")))?;

        let result = instance
            .thrustr_plugin_base()
            .call_get_login_flow(&mut store)
            .await
            .map_err(|e| ComponentError::Other(format!("Wasm call failed: {e}")))?
            .map_err(ComponentError::from)?;

        Ok(result
            .map(|flow| LoginMethod::Flow(flow.into()))
            .or_else(|| self.login_form.clone().map(LoginMethod::Form)))
    }

    async fn get_logout_flow(&self) -> Result<Option<AuthFlow>, ComponentError> {
        let (instance, mut store) = self
            .instantiate_storefront()
            .await
            .map_err(|e| ComponentError::Other(format!("{e:?}")))?;

        let result = instance
            .thrustr_plugin_base()
            .call_get_logout_flow(&mut store)
            .await
            .map_err(|e| ComponentError::Other(format!("Wasm call failed: {e}")))?
            .map_err(ComponentError::from)?;

        Ok(result.map(Into::into))
    }

    async fn login(
        &self,
        url: Option<String>,
        body: Option<String>,
        fields: Option<Vec<(String, String)>>,
    ) -> Result<(), ComponentError> {
        let (instance, mut store) = self
            .instantiate_storefront()
            .await
            .map_err(|e| ComponentError::Other(format!("{e:?}")))?;

        instance
            .thrustr_plugin_base()
            .call_login(
                &mut store,
                url.as_deref(),
                body.as_deref(),
                fields.as_deref(),
            )
            .await
            .map_err(|e| ComponentError::Other(format!("Wasm call failed: {e}")))?
            .map_err(ComponentError::from)
    }

    async fn logout(&self) -> Result<(), ComponentError> {
        let (instance, mut store) = self
            .instantiate_storefront()
            .await
            .map_err(|e| ComponentError::Other(format!("{e:?}")))?;

        instance
            .thrustr_plugin_base()
            .call_logout(&mut store)
            .await
            .map_err(|e| ComponentError::Other(format!("Wasm call failed: {e}")))?
            .map_err(ComponentError::from)
    }

    async fn validate_config(&self, fields: &[(String, String)]) -> Result<(), ComponentError> {
        let (instance, mut store) = self
            .instantiate_storefront()
            .await
            .map_err(|e| ComponentError::Other(format!("{e:?}")))?;

        instance
            .thrustr_plugin_base()
            .call_validate_config(&mut store, fields)
            .await
            .map_err(|e| ComponentError::Other(format!("Wasm call failed: {e}")))?
            .map_err(ComponentError::from)
    }
}

impl Capability for Plugin {
    fn component(self: Arc<Self>) -> Arc<dyn Component> {
        self as Arc<dyn Component>
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
            PluginError::Auth(msg) => ComponentError::Auth(msg),
            PluginError::Config(msg) => ComponentError::Config(msg),
            PluginError::Other(msg) => ComponentError::Other(msg),
        }
    }
}
