use crate::wit::exports::thrustr::plugin::base;
use crate::wit::thrustr::plugin::types::Error as PluginError;
use async_trait::async_trait;
use domain::component::{
    Auth, Component, Config, Error as ComponentError, Image, Metadata, Origin, Scheduler,
    Storefront,
};
use std::sync::Arc;

mod capabilities;
mod host;
mod manifest;
mod runtime;
mod state;

pub use capabilities::{PluginAuth, PluginConfig, PluginScheduler, PluginStorefront};
pub use host::http_client;
pub use manifest::*;
pub use runtime::PluginRuntime;
pub(crate) use runtime::guest_call;
pub use state::PluginState;

pub struct Plugin {
    pub(crate) info: PluginInfo,
    pub(crate) icon: Option<Image>,
    pub(crate) runtime: Arc<PluginRuntime>,
    pub(crate) base: base::GuestIndices,
    pub(crate) auth: Option<Arc<dyn Auth>>,
    pub(crate) config: Option<Arc<dyn Config>>,
    pub(crate) storefront: Option<Arc<dyn Storefront>>,
    pub(crate) scheduler: Option<Arc<dyn Scheduler>>,
}

#[async_trait]
impl Component for Plugin {
    fn metadata(&self) -> Metadata<'_> {
        Metadata {
            id: &self.info.id,
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
