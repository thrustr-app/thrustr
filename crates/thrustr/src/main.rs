use crate::routes::Root;
use assets::Assets;
use gpui::{AppContext, Application, TitlebarOptions, WindowOptions};
use plugin_manager::PluginManagerExt;
use sqlite_storage::SqliteStorage;
use std::sync::Arc;

mod components;
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
        Assets
            .load_fonts(cx)
            .expect("Failed to load embedded fonts");

        gpui_router::init(cx);
        gpui_tokio::init(cx);
        theme_manager::init(cx);
        plugin_manager::init(cx, storage);

        /*let plugin_manager = cx.plugin_manager();

        // TODO: For testing purposes for now.
        cx.background_executor()
            .block(plugin_manager.load_plugins_from_dir("target/plugins"))
            .unwrap();

        let plugin = plugin_manager.get_plugin("epic-games").unwrap();

        cx.background_spawn(async move {
            plugin.init().await.unwrap();
            plugin.auth("https://www.epicgames.com/id/api/redirect?clientId=34a02cf8f4414e29b15921876da36f9a&responseType=code", b"{\"warning\":\"Do not share this code with any 3rd party service. It allows full access to your Epic account.\",\"redirectUrl\":\"https://localhost/launcher/authorized?code=blahblahsomecode\",\"authorizationCode\":\"blahblahsomecode\",\"exchangeCode\":null,\"sid\":null}").await.unwrap();
        })
        .detach();
        // TODO-END.*/

        cx.activate(true);
        cx.open_window(
            WindowOptions {
                focus: true,
                app_id: Some("com.thrustr.thrustr".into()),
                titlebar: Some(TitlebarOptions {
                    title: Some("Thrustr".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| Root::new(window, cx)),
        )
        .unwrap();
    });
}
