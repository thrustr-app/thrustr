use crate::routes::Root;
use gpui::{AppContext, Application, TitlebarOptions, WindowOptions};
use plugin_manager::PluginManager;

#[path = "routes/root.rs"]
mod routes;

fn main() {
    let mut plugin_manager = PluginManager::new().unwrap();
    plugin_manager
        .load_plugins_from_dir("target/wasm-plugins")
        .unwrap();

    let plugins = plugin_manager.list_plugins();
    println!("Plugins: {:?}", plugins);

    let plugin = plugin_manager.get_plugin_mut("epic-games").unwrap();

    let manifest = plugin.get_manifest().unwrap();
    println!("Epic Games Manifest: {:?}", manifest);

    plugin.init().unwrap();

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
