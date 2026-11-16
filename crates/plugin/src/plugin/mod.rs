use crate::wit::exports::thrustr::plugin::base;
use crate::wit::thrustr::plugin::types::{Error as PluginError, FieldValue as PluginFieldValue};
use async_trait::async_trait;
use domain::component::{
    Auth, Component, Config, Error as ComponentError, FieldValue, Image, Metadata, Origin,
    Scheduler, Storefront,
};
use std::sync::Arc;

mod capabilities;
mod host;
mod manifest;
mod runtime;
mod state;
mod windows;

pub use capabilities::{PluginAuth, PluginConfig, PluginScheduler, PluginStorefront};
pub use host::Reporter;
pub use host::add_fs_to_linker;
pub use host::{AllowedHosts, PluginCache, http_client};
pub use manifest::{PluginId, PluginInfo, PluginManifest};
pub use runtime::PluginRuntime;
pub(crate) use runtime::{guest_call, guest_fn};
pub use state::{DownloadCall, PluginState};

pub struct Plugin {
    pub info: PluginInfo,
    pub icon: Option<Image>,
    pub runtime: Arc<PluginRuntime>,
    pub base: base::GuestIndices,
    pub auth: Option<Arc<dyn Auth>>,
    pub config: Option<Arc<dyn Config>>,
    pub storefront: Option<Arc<dyn Storefront>>,
    pub scheduler: Option<Arc<dyn Scheduler>>,
}

#[async_trait]
impl Component for Plugin {
    fn metadata(&self) -> Metadata<'_> {
        Metadata {
            id: self.info.id.as_str(),
            name: &self.info.name,
            description: self.info.description.as_deref(),
            version: &self.info.version,
            authors: &self.info.authors,
            icon: self.icon.as_ref(),
            origin: Origin::Plugin,
        }
    }

    fn auth(&self) -> Option<Arc<dyn Auth>> {
        self.auth.clone()
    }

    fn config(&self) -> Option<Arc<dyn Config>> {
        self.config.clone()
    }

    fn storefront(&self) -> Option<Arc<dyn Storefront>> {
        self.storefront.clone()
    }

    fn scheduler(&self) -> Option<Arc<dyn Scheduler>> {
        self.scheduler.clone()
    }

    async fn init(&self) -> Result<(), ComponentError> {
        guest_call!(self.runtime, self.base, |base, accessor| {
            base.call_init(accessor)
        })
        .await
    }
}

impl From<PluginError> for ComponentError {
    fn from(value: PluginError) -> Self {
        match value {
            PluginError::Auth(msg) => Self::Auth(msg),
            PluginError::Config(msg) => Self::Config(msg),
            PluginError::Other(msg) => Self::Other(msg),
        }
    }
}

impl From<FieldValue> for PluginFieldValue {
    fn from(value: FieldValue) -> Self {
        match value {
            FieldValue::Text(text) => Self::Text(text),
            FieldValue::List(values) => Self::List(values),
        }
    }
}
