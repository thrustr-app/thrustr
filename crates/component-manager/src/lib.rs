use dashmap::DashMap;
use ports::capabilities::Storefront;
use std::sync::Arc;

#[derive(Clone)]
pub struct ComponentManager {
    storefronts: Arc<DashMap<String, Arc<dyn Storefront>>>,
}

impl ComponentManager {
    pub fn new() -> Self {
        Self {
            storefronts: Arc::new(DashMap::new()),
        }
    }

    pub fn register_storefront(&self, storefront: Arc<dyn Storefront>) {
        self.storefronts
            .insert(storefront.metadata().id.to_owned(), storefront);
        event::emit("component");
    }

    pub fn storefronts(&self) -> Vec<Arc<dyn Storefront>> {
        self.storefronts.iter().map(|s| s.value().clone()).collect()
    }

    pub fn storefront(&self, id: &str) -> Option<Arc<dyn Storefront>> {
        self.storefronts.get(id).map(|s| s.value().clone())
    }
}
