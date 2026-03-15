#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use crate::app::App;
use assets::Assets;
use config::paths;
use gpui::{AppContext, TitlebarOptions, WindowOptions};
use sqlite_storage::SqliteStorage;
use std::sync::Arc;
use ui::UiProvider;

mod app;
mod components;
mod conversions;
mod globals;
mod navigation;
mod routes;
mod webview;

fn main() {
    let db_path = paths::db_path();
    let sqlite_storage = SqliteStorage::new(&db_path).expect(&format!(
        "Failed to initialize database at {}",
        db_path.display()
    ));
    let storage = Arc::new(sqlite_storage);

    gpui_platform::application()
        .with_assets(Assets)
        .run(move |cx| {
            Assets
                .load_fonts(cx)
                .expect("Failed to load embedded fonts");

            navigation::init(cx);
            gpui_tokio::init(cx);
            globals::init(cx, storage.clone(), storage);

            cx.activate(true);

            cx.spawn(async move |cx| {
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
                    |window, cx| {
                        let view = cx.new(|cx| App::new(window, cx));
                        UiProvider::new(view, window, cx)
                    },
                )
                .unwrap();
            })
            .detach();
        });
}
