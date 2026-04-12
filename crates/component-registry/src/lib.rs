use dashmap::DashMap;
use domain::component::Component;
use std::sync::Arc;

mod context;
mod handles;

pub use context::*;
pub use handles::*;

#[derive(Clone)]
pub struct ComponentRegistry {
    components: Arc<DashMap<String, Arc<dyn Component>>>,
    context: RegistryContext,
}

impl ComponentRegistry {
    pub fn new(context: RegistryContext) -> Self {
        Self {
            components: Arc::new(DashMap::new()),
            context,
        }
    }

    pub fn register(&self, component: Arc<dyn Component>) {
        self.components
            .insert(component.metadata().id.to_owned(), component);
    }

    pub fn component(&self, id: &str) -> Option<ComponentHandle> {
        self.components
            .get(id)
            .map(|c| ComponentHandle::new(Arc::clone(c.value()), self.context.clone()))
    }

    pub fn components(&self) -> Vec<ComponentHandle> {
        self.components
            .iter()
            .map(|c| ComponentHandle::new(Arc::clone(c.value()), self.context.clone()))
            .collect()
    }

    pub fn plugins(&self) -> Vec<ComponentHandle> {
        self.components
            .iter()
            .filter(|c| c.value().metadata().origin.is_plugin())
            .map(|c| ComponentHandle::new(Arc::clone(c.value()), self.context.clone()))
            .collect()
    }

    pub fn storefronts(&self) -> Vec<StorefrontHandle> {
        self.components
            .iter()
            .filter_map(|c| {
                Arc::clone(c.value())
                    .storefront()
                    .map(|s| StorefrontHandle::new(s, self.context.clone()))
            })
            .collect()
    }

    pub fn storefront(&self, id: &str) -> Option<StorefrontHandle> {
        self.components
            .get(id)
            .and_then(|c| Arc::clone(c.value()).storefront())
            .map(|s| StorefrontHandle::new(s, self.context.clone()))
    }
}
