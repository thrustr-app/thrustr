#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use crate::app::App;
use assets::Assets;
use config::{logging, paths, tls};
use gpui::{AppContext, CursorHideMode, TitlebarOptions, WindowDecorations, WindowOptions};
use sqlite::SqliteStorage;
use std::{fs, sync::Arc};
use tracing::warn;
use ui::{TRAFFIC_LIGHT_POSITION, UiProvider};

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

    let plugins_dir = paths::plugins_dir();
    if let Err(e) = fs::create_dir_all(&plugins_dir) {
        warn!(path = %plugins_dir.display(), error = %e, "failed to create plugins directory");
    }

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
                            appears_transparent: true,
                            traffic_light_position: Some(TRAFFIC_LIGHT_POSITION),
                        }),
                        app_owns_titlebar_drag: true,
                        window_decorations: cfg!(target_os = "linux")
                            .then_some(WindowDecorations::Client),
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
