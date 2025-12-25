use crate::routes::Root;
use gpui::{AppContext, Application, TitlebarOptions, WindowOptions};
use plugin_manager::PluginManager;
use sqlite_storage::SqliteStorage;
use std::sync::Arc;
use tokio::runtime;

#[path = "routes/root.rs"]
mod routes;

fn main() {
    let tokio_runtime = runtime::Builder::new_multi_thread().build().unwrap();

    // TODO
    let exe_path = std::env::current_exe().unwrap();
    let exe_dir = exe_path.parent().unwrap();
    let db_path = exe_dir.join("thrustr.db");
    let database_manager = SqliteStorage::new(db_path).unwrap();

    let mut plugin_manager =
        PluginManager::new(tokio_runtime.handle().clone(), Arc::new(database_manager));
    plugin_manager
        .load_plugins_from_dir("target/wasm-plugins")
        .unwrap();

    let plugins = plugin_manager.list_plugins();
    println!("Loaded Plugins: {:?}", plugins);
    // TODO

    Application::new().run(move |app| {
        gpui_tokio::init_from_handle(app, tokio_runtime.handle().clone());

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
