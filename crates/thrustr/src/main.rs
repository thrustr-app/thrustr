use crate::app::App;
use assets::Assets;
use config::paths;
use gpui::{AppContext, Application, TitlebarOptions, WindowOptions};
use sqlite_storage::SqliteStorage;
use std::sync::Arc;

mod app;
mod components;
mod conversions;
mod globals;
mod navigation;
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
        globals::init(cx, storage);

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
            |window, cx| cx.new(|cx| App::new(window, cx)),
        )
        .unwrap();
    });
}
