use crate::wit::exports::thrustr::plugin::base::{
    AuthFlow as PluginAuthFlow, Error as PluginError, LoginFlow, LoginForm,
    LoginRequest as PluginLoginRequest,
};
use crate::wit::{StorefrontPlugin, StorefrontPluginPre};
use async_trait::async_trait;
use domain::component::capabilities::Storefront;
use domain::component::{
    AuthFlow, Component, ComponentConfig, ComponentStorage, Error as ComponentError, Image,
    LoginMethod, LoginRequest, Metadata, Origin,
};
use reqwest::Client;
use runtime::TokioHandle;
use std::collections::HashMap;
use std::sync::Arc;
use wasmtime::{Engine, ResourceLimiter, Store};

mod capabilities;
mod host;
mod manifest;
mod state;

pub use host::http_client;
pub use manifest::*;
pub use state::PluginState;

pub struct Plugin {
    pub manifest: PluginManifest,
    pub icon: Option<Image>,
    pub engine: Engine,
    pub storage: Arc<dyn ComponentStorage>,
    pub storefront_pre: Option<StorefrontPluginPre<PluginState>>,
    pub tokio_handle: TokioHandle,
    pub http_client: Client,
    pub allowed_hosts: Arc<[String]>,
}

type CallResult<R> = wasmtime::Result<Result<R, PluginError>>;

const MAX_FUEL: u64 = 10_000_000_000;
const FUEL_YIELD_INTERVAL: u64 = 10_000_000;

impl Plugin {
    async fn call<R, F, Fut>(&self, f: F) -> Result<R, ComponentError>
    where
        R: Send + 'static,
        F: FnOnce(StorefrontPlugin, Store<PluginState>) -> Fut + Send + 'static,
        Fut: Future<Output = CallResult<R>> + Send + 'static,
    {
        let pre = self
            .storefront_pre
            .clone()
            .ok_or_else(|| ComponentError::Other("Not a storefront".into()))?;

        let engine = self.engine.clone();
        let storage = self.storage.clone();
        let id = self.manifest.plugin.id.clone();
        let http_client = self.http_client.clone();
        let allowed_hosts = self.allowed_hosts.clone();

        self.tokio_handle
            .spawn(async move {
                let state = PluginState::new(&id, storage, http_client, allowed_hosts);
                let mut store = Store::new(&engine, state);

                store.limiter(|state| state.limits() as &mut dyn ResourceLimiter);

                store
                    .set_fuel(MAX_FUEL)
                    .and_then(|()| store.fuel_async_yield_interval(Some(FUEL_YIELD_INTERVAL)))
                    .map_err(|e| ComponentError::Other(format!("Fuel setup failed: {e}")))?;

                let instance = pre
                    .instantiate_async(&mut store)
                    .await
                    .map_err(|e| ComponentError::Other(format!("Instantiation failed: {e}")))?;

                f(instance, store)
                    .await
                    .map_err(|e| ComponentError::Other(format!("Wasm call failed: {e}")))?
                    .map_err(ComponentError::from)
            })
            .await
            .map_err(|e| ComponentError::Other(format!("Plugin task failed: {e}")))?
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
        self.call(|instance, mut store| async move {
            store
                .run_concurrent(async |accessor| {
                    instance.thrustr_plugin_base().call_init(accessor).await
                })
                .await
                .and_then(|result| result)
        })
        .await
    }

    async fn login_method(&self) -> Result<Option<LoginMethod>, ComponentError> {
        let flow = self
            .call(|instance, mut store| async move {
                store
                    .run_concurrent(async |accessor| {
                        instance
                            .thrustr_plugin_base()
                            .call_get_login_flow(accessor)
                            .await
                    })
                    .await
                    .and_then(|result| result)
            })
            .await?;

        Ok(flow
            .map(|flow| LoginMethod::Flow(flow.into()))
            .or_else(|| self.manifest.auth.clone().map(LoginMethod::Form)))
    }

    async fn logout_flow(&self) -> Result<Option<AuthFlow>, ComponentError> {
        let flow = self
            .call(|instance, mut store| async move {
                store
                    .run_concurrent(async |accessor| {
                        instance
                            .thrustr_plugin_base()
                            .call_get_logout_flow(accessor)
                            .await
                    })
                    .await
                    .and_then(|result| result)
            })
            .await?;

        Ok(flow.map(Into::into))
    }

    async fn login(&self, request: LoginRequest) -> Result<(), ComponentError> {
        self.call(|instance, mut store| async move {
            store
                .run_concurrent(async |accessor| {
                    instance
                        .thrustr_plugin_base()
                        .call_login(accessor, request.into())
                        .await
                })
                .await
                .and_then(|result| result)
        })
        .await
    }

    async fn logout(&self) -> Result<(), ComponentError> {
        self.call(|instance, mut store| async move {
            store
                .run_concurrent(async |accessor| {
                    instance.thrustr_plugin_base().call_logout(accessor).await
                })
                .await
                .and_then(|result| result)
        })
        .await
    }

    async fn validate_config(&self, fields: HashMap<String, String>) -> Result<(), ComponentError> {
        self.call(|instance, mut store| async move {
            store
                .run_concurrent(async |accessor| {
                    instance
                        .thrustr_plugin_base()
                        .call_validate_config(accessor, fields)
                        .await
                })
                .await
                .and_then(|result| result)
        })
        .await
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

impl From<LoginRequest> for PluginLoginRequest {
    fn from(value: LoginRequest) -> Self {
        match value {
            LoginRequest::Flow { url, body } => PluginLoginRequest::Flow(LoginFlow { url, body }),
            LoginRequest::Form { fields } => PluginLoginRequest::Form(LoginForm { fields }),
        }
    }
}
