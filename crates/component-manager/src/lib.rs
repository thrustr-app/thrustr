use dashmap::DashMap;
use ports::{capabilities::Storefront, component::Component, storage::ComponentStorage};
use std::sync::Arc;

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

    pub fn component(&self, component_id: &str) -> Option<Arc<dyn Component>> {
        self.components
            .get(component_id)
            .map(|c| Arc::clone(c.value()))
    }

    pub fn get_config_values(&self, component_id: &str) -> Vec<(String, String)> {
        self.storage.get_config_values(component_id).unwrap()
    }

    pub fn save_config_values(&self, component_id: &str, fields: &[(String, String)]) {
        self.storage
            .set_config_values(component_id, fields)
            .unwrap();
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
