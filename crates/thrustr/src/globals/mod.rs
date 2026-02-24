use gpui::App;
use ports::storage::ExtensionStorage;
use std::sync::Arc;

mod plugin;
mod storefront;
mod theme;

pub use plugin::PluginManagerExt;
pub use storefront::StorefrontManagerExt;

pub fn init(cx: &mut App, storage: Arc<dyn ExtensionStorage>) {
    theme::init(cx);
    let storefront_manager = storefront::init(cx);
    plugin::init(cx, storage, storefront_manager);
}
