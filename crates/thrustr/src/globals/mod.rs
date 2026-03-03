use gpui::App;
use ports::storage::ComponentStorage;
use std::sync::Arc;

mod event;
mod plugin;
mod storefront;
mod theme;

pub use event::EventListenerExt;
pub use plugin::PluginManagerExt;
pub use storefront::StorefrontManagerExt;

pub fn init(cx: &mut App, storage: Arc<dyn ComponentStorage>) {
    theme::init(cx);
    let storefront_manager = storefront::init(cx);
    plugin::init(cx, storage, storefront_manager);
}
