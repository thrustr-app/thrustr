use component_manager::{ComponentHandle, ComponentManager, StorefrontHandle};
use gpui::{App, Global};
use ports::storage::ComponentStorage;
use std::sync::Arc;

pub(super) struct ComponentManagerGlobal(ComponentManager);

impl Global for ComponentManagerGlobal {}

pub(super) fn init(cx: &mut App, storage: Arc<dyn ComponentStorage>) -> Arc<ComponentManager> {
    let manager = ComponentManager::new(storage);
    cx.set_global(ComponentManagerGlobal(manager.clone()));
    Arc::new(manager)
}

pub trait ComponentManagerExt {
    fn component_manager(&self) -> ComponentManager;

    fn component(&self, id: &str) -> Option<ComponentHandle> {
        self.component_manager().component(id)
    }

    fn components(&self) -> Vec<ComponentHandle> {
        self.component_manager().components()
    }

    fn storefronts(&self) -> Vec<StorefrontHandle> {
        self.component_manager().storefronts()
    }
}

impl ComponentManagerExt for App {
    fn component_manager(&self) -> ComponentManager {
        self.global::<ComponentManagerGlobal>().0.clone()
    }
}
