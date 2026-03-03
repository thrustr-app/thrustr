use component_manager::ComponentManager;
use gpui::{App, Global};
use ports::capabilities::Storefront;
use std::sync::Arc;

pub(super) struct ComponentManagerGlobal(ComponentManager);

impl Global for ComponentManagerGlobal {}

pub(super) fn init(cx: &mut App) -> Arc<ComponentManager> {
    let manager = ComponentManager::new();
    cx.set_global(ComponentManagerGlobal(manager.clone()));
    Arc::new(manager)
}

pub trait ComponentManagerExt {
    fn component_manager(&self) -> ComponentManager;
    fn storefronts(&self) -> Vec<Arc<dyn Storefront>>;
}

impl ComponentManagerExt for App {
    fn component_manager(&self) -> ComponentManager {
        self.global::<ComponentManagerGlobal>().0.clone()
    }

    fn storefronts(&self) -> Vec<Arc<dyn Storefront>> {
        self.component_manager().storefronts()
    }
}
