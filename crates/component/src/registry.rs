use crate::{ComponentHandle, StorefrontHandle};
use artwork::ArtworkService;
use dashmap::{DashMap, Entry};
use domain::{
    component::{Component, ComponentStorage},
    game::GameRepository,
};
use runtime::TokioHandle;
use std::sync::Arc;
use thiserror::Error;

#[derive(Clone)]
pub struct RegistryContext {
    pub tokio_handle: TokioHandle,
    pub component_storage: Arc<dyn ComponentStorage>,
    pub game_repository: Arc<dyn GameRepository>,
    pub artwork_service: ArtworkService,
}

#[derive(Debug, Error)]
#[error("component `{id}` is already registered")]
pub struct DuplicateComponentError {
    pub id: String,
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

    pub fn register(
        &self,
        component: Arc<dyn Component>,
    ) -> Result<ComponentHandle, DuplicateComponentError> {
        let id = component.metadata().id.to_owned();
        match self.components.entry(id) {
            Entry::Occupied(entry) => Err(DuplicateComponentError {
                id: entry.key().clone(),
            }),
            Entry::Vacant(entry) => {
                let handle = ComponentHandle::new(component, self.context.clone());
                entry.insert(handle.clone());
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
