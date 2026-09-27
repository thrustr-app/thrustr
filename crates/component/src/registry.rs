use crate::{ComponentHandle, StorefrontHandle};
use artwork::ArtworkService;
use dashmap::{DashMap, Entry};
use domain::{
    component::{Capabilities, Component, ComponentStorage, Error as ComponentError},
    game::GameRepository,
};
use runtime::TokioHandle;
use std::sync::Arc;
use thiserror::Error;
use tracing::debug;

#[derive(Clone)]
pub struct RegistryContext {
    pub tokio_handle: TokioHandle,
    pub component_storage: Arc<dyn ComponentStorage>,
    pub game_repository: Arc<dyn GameRepository>,
    pub artwork_service: ArtworkService,
}

#[derive(Debug, Error)]
pub enum RegisterError {
    #[error("component `{id}` is already registered")]
    Duplicate { id: String },

    #[error(transparent)]
    Component(#[from] ComponentError),
}

#[derive(Clone)]
pub struct ComponentRegistry {
    components: Arc<DashMap<String, ComponentHandle>>,
    context: RegistryContext,
}

impl ComponentRegistry {
    pub fn new(context: RegistryContext) -> Self {
        Self {
            components: Arc::new(DashMap::new()),
            context,
        }
    }

    pub async fn register(
        &self,
        component: Arc<dyn Component>,
    ) -> Result<ComponentHandle, RegisterError> {
        let id = component.metadata().id.to_owned();
        if self.components.contains_key(&id) {
            return Err(RegisterError::Duplicate { id });
        }

        let capabilities = Capabilities {
            login: component.login_method().await?.is_some(),
            config: component.config().is_some(),
            storefront: Arc::clone(&component).storefront().is_some(),
        };

        match self.components.entry(id) {
            Entry::Occupied(entry) => Err(RegisterError::Duplicate {
                id: entry.key().clone(),
            }),
            Entry::Vacant(entry) => {
                let handle = ComponentHandle::new(component, self.context.clone(), capabilities);
                entry.insert(handle.clone());
                debug!(component = handle.id(), "component registered");
                Ok(handle)
            }
        }
    }

    pub fn component(&self, id: &str) -> Option<ComponentHandle> {
        self.components.get(id).map(|c| c.value().clone())
    }

    pub fn components(&self) -> Vec<ComponentHandle> {
        self.components.iter().map(|c| c.value().clone()).collect()
    }

    pub fn storefront(&self, id: &str) -> Option<StorefrontHandle> {
        self.components.get(id).and_then(|c| c.value().storefront())
    }

    pub fn storefronts(&self) -> Vec<StorefrontHandle> {
        self.components
            .iter()
            .filter_map(|c| c.value().storefront())
            .collect()
    }
}
