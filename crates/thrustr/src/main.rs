use crate::routes::Root;
use assets::Assets;
use gpui::{AppContext, Application, TitlebarOptions, WindowOptions};
use sqlite_storage::SqliteStorage;
use tokio::runtime;

#[path = "routes/root.rs"]
mod routes;

fn main() {
    let tokio_runtime = runtime::Builder::new_multi_thread()
        .build()
        .expect("Failed to create async runtime");

    let db_path = paths::db_path();
    let _database_manager = SqliteStorage::new(&db_path).expect(&format!(
        "Failed to initialize database at {}",
        db_path.display()
    ));

    // TODO
    /*let mut plugin_manager =
        PluginManager::new(tokio_runtime.handle().clone(), Arc::new(database_manager));
    plugin_manager
        .load_plugins_from_dir("target/wasm-plugins")
        .unwrap();

    let plugins = plugin_manager.list_plugins();
    println!("Loaded Plugins: {:?}", plugins);*/

    Application::new().with_assets(Assets).run(move |cx| {
        gpui_tokio::init_from_handle(cx, tokio_runtime.handle().clone());
        theme_manager::init(cx);
        plugin_manager::init(cx);

        cx.open_window(
            WindowOptions {
                focus: true,
                titlebar: Some(TitlebarOptions {
                    title: Some("Thrustr".into()),
                    appears_transparent: true,
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| Root::new(window, cx)),
        )
        .unwrap();
    });
}
