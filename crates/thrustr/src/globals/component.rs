use application::component::{
    ComponentHandle, ComponentRegistry, RegistryContext, StorefrontHandle,
};
use domain::{component::ComponentStorage, game::GameRepository};
use gpui::{App, Global};
use std::sync::Arc;

pub(super) struct ComponentRegistryGlobal(ComponentRegistry);

impl Global for ComponentRegistryGlobal {}

pub(super) fn init(
    cx: &mut App,
    component_storage: Arc<dyn ComponentStorage>,
    game_storage: Arc<dyn GameRepository>,
) -> ComponentRegistry {
    let registry = ComponentRegistry::new(RegistryContext {
        component_storage,
        game_storage,
    });
    cx.set_global(ComponentRegistryGlobal(registry.clone()));
    registry
}

pub trait ComponentRegistryExt {
    fn component_registry(&self) -> ComponentRegistry;

    fn component(&self, id: &str) -> Option<ComponentHandle> {
        self.component_registry().component(id)
    }

    fn components(&self) -> Vec<ComponentHandle> {
        self.component_registry().components()
    }

    fn storefronts(&self) -> Vec<StorefrontHandle> {
        self.component_registry().storefronts()
    }
}

impl ComponentRegistryExt for App {
    fn component_registry(&self) -> ComponentRegistry {
        self.global::<ComponentRegistryGlobal>().0.clone()
    }
}
