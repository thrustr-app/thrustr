use dashmap::DashMap;
use domain::{component::Component, storage::ComponentStorage};
use std::sync::Arc;

mod handles;

pub use handles::*;

#[derive(Clone)]
pub struct ComponentManager {
    components: Arc<DashMap<String, Arc<dyn Component>>>,
    storage: Arc<dyn ComponentStorage>,
}

impl ComponentManager {
    pub fn new(storage: Arc<dyn ComponentStorage>) -> Self {
        Self {
            components: Arc::new(DashMap::new()),
            storage: storage,
        }
    }

    pub fn register(&self, component: Arc<dyn Component>) {
        self.components
            .insert(component.metadata().id.to_owned(), component);
    }

    pub fn component(&self, id: &str) -> Option<ComponentHandle> {
        self.components
            .get(id)
            .map(|c| ComponentHandle::new(Arc::clone(c.value()), Arc::clone(&self.storage)))
    }

    pub fn components(&self) -> Vec<ComponentHandle> {
        self.components
            .iter()
            .map(|c| ComponentHandle::new(Arc::clone(c.value()), Arc::clone(&self.storage)))
            .collect()
    }

    pub fn plugins(&self) -> Vec<ComponentHandle> {
        self.components
            .iter()
            .filter(|c| c.value().metadata().origin.is_plugin())
            .map(|c| ComponentHandle::new(Arc::clone(c.value()), Arc::clone(&self.storage)))
            .collect()
    }

    pub fn storefronts(&self) -> Vec<StorefrontHandle> {
        self.components
            .iter()
            .filter_map(|c| {
                Arc::clone(c.value())
                    .storefront()
                    .map(|s| StorefrontHandle::new(s, Arc::clone(&self.storage)))
            })
            .collect()
    }

    pub fn storefront(&self, id: &str) -> Option<StorefrontHandle> {
        self.components
            .get(id)
            .and_then(|c| Arc::clone(c.value()).storefront())
            .map(|s| StorefrontHandle::new(s, Arc::clone(&self.storage)))
    }
}
