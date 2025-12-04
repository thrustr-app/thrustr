use crate::routes::Root;
use gpui::{AppContext, Application, TitlebarOptions, WindowOptions};

#[path = "routes/root.rs"]
mod routes;

fn main() {
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
