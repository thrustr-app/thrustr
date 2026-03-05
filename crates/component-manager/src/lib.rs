use dashmap::DashMap;
use ports::{capabilities::Storefront, component::Component};
use std::sync::Arc;

#[derive(Clone)]
pub struct ComponentManager {
    components: Arc<DashMap<String, Arc<dyn Component>>>,
}

impl ComponentManager {
    pub fn new() -> Self {
        Self {
            components: Arc::new(DashMap::new()),
        }
    }

    pub fn register(&self, component: Arc<dyn Component>) {
        self.components
            .insert(component.metadata().id.to_owned(), component);
    }

    pub fn plugins(&self) -> Vec<Arc<dyn Component>> {
        self.components
            .iter()
            .filter(|c| c.value().metadata().origin.is_plugin())
            .map(|c| Arc::clone(c.value()))
            .collect()
    }

    pub fn storefronts(&self) -> Vec<Arc<dyn Storefront>> {
        self.components
            .iter()
            .filter_map(|c| {
                let component = Arc::clone(c.value());
                component.storefront()
            })
            .collect()
    }

    pub fn storefront(&self, id: &str) -> Option<Arc<dyn Storefront>> {
        self.components
            .get(id)
            .and_then(|c| Arc::clone(c.value()).storefront())
    }
}
