#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use crate::app::App;
use assets::Assets;
use config::{logging, paths, tls};
use gpui::{AppContext, CursorHideMode, TitlebarOptions, WindowOptions};
use sqlite::SqliteStorage;
use std::sync::Arc;
use ui::UiProvider;

mod app;
mod conversions;
mod extensions;
mod globals;
mod navigation;
mod routes;
mod tokio;
mod webview;

fn main() {
    let _guard = logging::init();
    tls::init();

    let db_path = paths::db_path();
    let sqlite_storage = SqliteStorage::new(&db_path)
        .unwrap_or_else(|_| panic!("Failed to initialize database at {}", db_path.display()));
    let storage = Arc::new(sqlite_storage);

    gpui_platform::application()
        .with_assets(Assets)
        .run(move |cx| {
            Assets
                .load_fonts(cx)
                .expect("Failed to load embedded fonts");

            cx.set_cursor_hide_mode(CursorHideMode::Never);

            theme::init(cx);
            navigation::init(cx);
            tokio::init(cx);

            globals::init(cx, storage);

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
