use gpui::{App, Global};
use plugin_manager::PluginManager;
use ports::storage::ComponentStorage;
use std::sync::Arc;
use storefront_manager::StorefrontManager;

pub struct PluginManagerGlobal(PluginManager);

impl Global for PluginManagerGlobal {}

pub fn init(
    cx: &mut App,
    storage: Arc<dyn ComponentStorage>,
    storefront_manager: Arc<StorefrontManager>,
) {
    cx.set_global(PluginManagerGlobal(PluginManager::new(
        storage,
        storefront_manager,
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
