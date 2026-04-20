use dashmap::DashMap;
use domain::{
    component::{Component, ComponentStorage},
    game::GameRepository,
};
use std::sync::Arc;

use crate::{ComponentHandle, StorefrontHandle};

#[derive(Clone)]
pub struct RegistryContext {
    pub component_storage: Arc<dyn ComponentStorage>,
    pub game_repository: Arc<dyn GameRepository>,
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

    pub fn register(&self, component: Arc<dyn Component>) -> ComponentHandle {
        let id = component.metadata().id.to_owned();
        let handle = ComponentHandle::new(component, self.context.clone());
        self.components.insert(id, handle.clone());
        handle
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
