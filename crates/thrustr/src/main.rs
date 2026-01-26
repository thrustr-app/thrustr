use crate::routes::Root;
use assets::Assets;
use gpui::{AppContext, Application, TitlebarOptions, WindowOptions};
use plugin_manager::PluginManagerExt;
use sqlite_storage::SqliteStorage;
use std::sync::Arc;

#[path = "routes/root.rs"]
mod routes;

fn main() {
    let db_path = paths::db_path();
    let sqlite_storage = SqliteStorage::new(&db_path).expect(&format!(
        "Failed to initialize database at {}",
        db_path.display()
    ));
    let storage = Arc::new(sqlite_storage);

    Application::new().with_assets(Assets).run(move |cx| {
        gpui_tokio::init(cx);
        theme_manager::init(cx);
        plugin_manager::init(cx, storage);

        let plugin_manager = cx.plugin_manager();

        // TODO: For testing purposes for now.
        cx.background_executor()
            .block(plugin_manager.load_plugins_from_dir("target/plugins"))
            .unwrap();

        let plugin = plugin_manager.get_plugin("epic-games").unwrap();

        cx.background_spawn(async move {
            plugin.init().await.unwrap();
        })
        .detach();
        // TODO-END.

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
