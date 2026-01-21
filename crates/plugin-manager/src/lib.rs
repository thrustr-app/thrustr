use gpui::{App, Global};

pub fn init(cx: &mut App) {
    cx.set_global(PluginManager);
}

pub struct PluginManager;

impl Global for PluginManager {}

pub trait PluginManagerExt {
    fn plugin_manager(&self) -> &PluginManager;
}

impl PluginManagerExt for App {
    fn plugin_manager(&self) -> &PluginManager {
        self.global::<PluginManager>()
    }
}
