use domain::storage::{ComponentStorage, GameStorage};
use gpui::App;
use std::sync::Arc;
use theme_manager::ThemeManager;

mod component;
mod plugin;

pub use component::ComponentRegistryExt;
pub use plugin::PluginManagerExt;

pub fn init(
    cx: &mut App,
    component_storage: Arc<dyn ComponentStorage>,
    game_storage: Arc<dyn GameStorage>,
) {
    cx.set_global(ThemeManager::new());

    let storefront_manager = component::init(cx, component_storage.clone(), game_storage.clone());
    plugin::init(cx, component_storage, storefront_manager);
}
