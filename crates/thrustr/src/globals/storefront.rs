use gpui::{App, Global};
use ports::managers::StorefrontManager as StorefrontManagerTrait;
use std::sync::Arc;
use storefront_manager::StorefrontManager;

pub(super) struct StorefrontManagerGlobal(StorefrontManager);

impl Global for StorefrontManagerGlobal {}

pub(super) fn init(cx: &mut App) -> Arc<dyn StorefrontManagerTrait> {
    let manager = StorefrontManager::new();
    cx.set_global(StorefrontManagerGlobal(manager.clone()));
    Arc::new(manager)
}

pub trait StorefrontManagerExt {
    fn storefront_manager(&self) -> StorefrontManager;
}

impl StorefrontManagerExt for App {
    fn storefront_manager(&self) -> StorefrontManager {
        self.global::<StorefrontManagerGlobal>().0.clone()
    }
}
