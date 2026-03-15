use domain::storage::ComponentStorage;
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

pub fn init(cx: &mut App, storage: Arc<dyn ComponentStorage>) {
    theme::init(cx);
    let storefront_manager = component::init(cx, storage.clone());
    plugin::init(cx, storage, storefront_manager);
}
