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
    storefronts: Arc<DashMap<String, Arc<dyn StorefrontProvider>>>,
}

impl StorefrontManager {
    pub fn new() -> Self {
        Self {
            storefronts: Arc::new(DashMap::new()),
        }
    }
}

impl StorefrontManagerTrait for StorefrontManager {
    fn register_storefront(&self, storefront: Arc<dyn StorefrontProvider>) {
        self.storefronts
            .insert(storefront.metadata().id, storefront);
    }

    fn storefronts(&self) -> Vec<Arc<dyn StorefrontProvider>> {
        self.storefronts.iter().map(|s| s.value().clone()).collect()
    }

    fn storefront(&self, id: &str) -> Option<Arc<dyn StorefrontProvider>> {
        self.storefronts.get(id).map(|s| s.value().clone())
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
