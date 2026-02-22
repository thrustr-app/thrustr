use dashmap::DashMap;
use gpui::{App, Global};
use ports::{managers::StorefrontManager as StorefrontManagerTrait, providers::StorefrontProvider};
use std::sync::Arc;

pub fn init(cx: &mut App) -> Arc<dyn StorefrontManagerTrait> {
    let manager = StorefrontManager::new();
    cx.set_global(manager.clone());
    Arc::new(manager)
}

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
            .insert(storefront.metadata().id, storefront);
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

impl Global for StorefrontManager {}

pub trait StorefrontManagerExt {
    fn storefront_manager(&self) -> StorefrontManager;
}

impl StorefrontManagerExt for App {
    fn storefront_manager(&self) -> StorefrontManager {
        self.global::<StorefrontManager>().clone()
    }
}
