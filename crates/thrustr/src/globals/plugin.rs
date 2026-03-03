use component_manager::ComponentManager;
use gpui::{App, Global};
use plugin_manager::PluginManager;
use ports::storage::ComponentStorage;
use std::sync::Arc;

pub struct PluginManagerGlobal(PluginManager);

impl Global for PluginManagerGlobal {}

pub fn init(
    cx: &mut App,
    storage: Arc<dyn ComponentStorage>,
    component_manager: Arc<ComponentManager>,
) {
    cx.set_global(PluginManagerGlobal(PluginManager::new(
        storage,
        component_manager,
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
