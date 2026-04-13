use crate::wit::exports::thrustr::plugin::base::{
    AuthFlow as PluginAuthFlow, Error as PluginError,
};
use crate::wit::{StorefrontPlugin, StorefrontPluginPre};
use async_trait::async_trait;
use domain::component::capabilities::Storefront;
use domain::component::{
    AuthFlow, Component, ComponentConfig, ComponentStorage, Error as ComponentError, Image,
    LoginMethod, Metadata, Origin,
};
use std::sync::Arc;
use wasmtime::{Engine, Store};

mod capabilities;
mod host;
mod manifest;
mod state;

pub use manifest::*;
pub use state::PluginState;

pub struct Plugin {
    pub manifest: PluginManifest,
    pub icon: Option<Image>,
    pub engine: Engine,
    pub storage: Arc<dyn ComponentStorage>,
    pub storefront_pre: Option<StorefrontPluginPre<PluginState>>,
}

impl Plugin {
    async fn instantiate_storefront(
        &self,
    ) -> Result<(StorefrontPlugin, Store<PluginState>), PluginError> {
        let pre = self
            .storefront_pre
            .as_ref()
            .ok_or(PluginError::Other("Not a storefront".into()))?;

        let mut store = Store::new(
            &self.engine,
            PluginState::new(&self.manifest.plugin.id, self.storage.clone()),
        );

        let instance = pre
            .instantiate_async(&mut store)
            .await
            .map_err(|e| PluginError::Other(format!("Instantiation failed: {e}")))?;

        Ok((instance, store))
    }
}

#[async_trait]
impl Component for Plugin {
    fn metadata(&self) -> Metadata<'_> {
        Metadata {
            id: &self.manifest.plugin.id,
            name: &self.manifest.plugin.name,
            description: self.manifest.plugin.description.as_deref(),
            version: &self.manifest.plugin.version,
            authors: &self.manifest.plugin.authors,
            icon: self.icon.as_ref(),
            origin: Origin::Plugin,
        }
    }

    fn config(&self) -> Option<ComponentConfig> {
        self.manifest.config.clone()
    }

    fn storefront(self: Arc<Self>) -> Option<Arc<dyn Storefront>> {
        self.storefront_pre.as_ref()?;
        Some(self as Arc<dyn Storefront>)
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
            .or_else(|| self.manifest.auth.clone().map(LoginMethod::Form)))
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
