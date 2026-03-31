use application::component::ComponentStorage;
use component_registry::ComponentRegistry;
use gpui::{App, Global};
use plugin_manager::PluginManager;
use std::sync::Arc;

pub struct PluginManagerGlobal(PluginManager);

impl Global for PluginManagerGlobal {}

pub fn init(
    cx: &mut App,
    storage: Arc<dyn ComponentStorage>,
    component_registry: Arc<ComponentRegistry>,
) {
    cx.set_global(PluginManagerGlobal(PluginManager::new(
        storage,
        component_registry,
    )));
}

pub trait PluginManagerExt {
    fn plugin_manager(&self) -> PluginManager;
}

impl PluginManagerExt for App {
    fn plugin_manager(&self) -> PluginManager {
        self.global::<PluginManagerGlobal>().0.clone()
    }
}
