use crate::tokio::Tokio;
use component::ComponentRegistry;
use domain::component::ComponentStorage;
use gpui::{App, Global};
use plugin::PluginService;
use std::sync::Arc;

pub struct PluginServiceGlobal(PluginService);

impl Global for PluginServiceGlobal {}

pub fn init(
    cx: &mut App,
    storage: Arc<dyn ComponentStorage>,
    component_registry: ComponentRegistry,
) {
    let handle = Tokio::handle(cx);

    cx.set_global(PluginServiceGlobal(PluginService::new(
        storage,
        component_registry,
        handle,
    )));
}

pub trait PluginServiceExt {
    fn plugin_service(&self) -> PluginService;
}

impl PluginServiceExt for App {
    fn plugin_service(&self) -> PluginService {
        self.global::<PluginServiceGlobal>().0.clone()
    }
}
