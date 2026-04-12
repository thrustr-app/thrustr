use gpui::App;
use sqlite_storage::SqliteStorage;
use std::sync::Arc;
use theme_manager::ThemeManager;

mod component;
mod game;
mod plugin;

pub use component::ComponentRegistryExt;
pub use game::GameServiceExt;
pub use plugin::PluginManagerExt;

pub fn init(cx: &mut App, storage: Arc<SqliteStorage>) {
    cx.set_global(ThemeManager::new());

    let registry = component::init(cx, storage.clone(), storage.clone());
    plugin::init(cx, storage.clone(), registry);
    game::init(cx, storage);
}
