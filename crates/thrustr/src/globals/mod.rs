use domain::storage::{ComponentStorage, GameStorage};
use gpui::App;
use std::sync::Arc;

mod component;
mod event;
mod plugin;
mod task;
mod theme;

pub use component::ComponentManagerExt;
pub use event::EventListenerExt;
pub use plugin::PluginManagerExt;
pub use task::*;

pub fn init(
    cx: &mut App,
    component_storage: Arc<dyn ComponentStorage>,
    game_storage: Arc<dyn GameStorage>,
) {
    theme::init(cx);
    let storefront_manager = component::init(cx, component_storage.clone(), game_storage.clone());
    plugin::init(cx, component_storage, storefront_manager);
}
