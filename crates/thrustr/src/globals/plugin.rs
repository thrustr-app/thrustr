use gpui::{App, Global};
use plugin_manager::PluginManager;
use ports::{managers::StorefrontManager, storage::ExtensionStorage};
use std::sync::Arc;

pub struct PluginManagerGlobal(PluginManager);

impl Global for PluginManagerGlobal {}

pub fn init(
    cx: &mut App,
    storage: Arc<dyn ExtensionStorage>,
    storefront_manager: Arc<dyn StorefrontManager>,
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
