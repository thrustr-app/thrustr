use std::sync::Arc;

use crate::routes::Root;
use gpui::{AppContext, Application, TitlebarOptions, WindowOptions};
use plugin_manager::PluginManager;
use sqlite_storage::SqliteStorage;

#[path = "routes/root.rs"]
mod routes;

fn main() {
    let exe_path = std::env::current_exe().unwrap();
    let exe_dir = exe_path.parent().unwrap();
    let db_path = exe_dir.join("thrustr.db");
    let database_manager = SqliteStorage::new(db_path).unwrap();

    let mut plugin_manager = PluginManager::new(Arc::new(database_manager));
    plugin_manager
        .load_plugins_from_dir("target/wasm-plugins")
        .unwrap();

    let plugins = plugin_manager.list_plugins();
    println!("Loaded Plugins: {:?}", plugins);

    Application::new().run(|app| {
        app.open_window(
            WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some("Thrustr".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, app| app.new(|cx| Root::new(window, cx)),
        )
        .unwrap();
    });
}
