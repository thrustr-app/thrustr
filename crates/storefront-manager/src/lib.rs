use dashmap::DashMap;
use ports::{managers::StorefrontManager as StorefrontManagerTrait, providers::StorefrontProvider};
use std::sync::Arc;

#[derive(Clone)]
pub struct StorefrontManager {
    storefront_providers: Arc<DashMap<String, Arc<dyn StorefrontProvider>>>,
}

impl StorefrontManager {
    pub fn new() -> Self {
        Self {
            storefront_providers: Arc::new(DashMap::new()),
        }
    }
}

impl StorefrontManagerTrait for StorefrontManager {
    fn register_storefront_provider(&self, storefront: Arc<dyn StorefrontProvider>) {
        self.storefront_providers
            .insert(storefront.id().to_string(), storefront);
    }

    fn storefront_providers(&self) -> Vec<Arc<dyn StorefrontProvider>> {
        self.storefront_providers
            .iter()
            .map(|s| s.value().clone())
            .collect()
    }

    fn storefront_provider(&self, id: &str) -> Option<Arc<dyn StorefrontProvider>> {
        self.storefront_providers.get(id).map(|s| s.value().clone())
    }
}
