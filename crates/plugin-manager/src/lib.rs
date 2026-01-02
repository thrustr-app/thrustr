use dashmap::DashMap;
use domain::Storefront;
use epic_games::EpicGames;
use gpui::{App, Global};

// IMPORTANT TODO: this is a temporary implementation until wasi preview 3 / threads are supported
// and we can load plugins dynamically.

struct Plugin {
    storefront: Box<dyn Storefront + Send + Sync>,
    error: Option<String>,
}

pub fn init(cx: &mut App) {
    let epic_games = EpicGames;

    let plugins: DashMap<String, Plugin> = DashMap::new();

    let epic_plugin = Plugin {
        storefront: Box::new(epic_games),
        error: None,
    };
    plugins.insert("epic_games".to_string(), epic_plugin);

    cx.set_global(PluginManager { plugins });
}

pub struct PluginManager {
    plugins: DashMap<String, Plugin>,
}

impl PluginManager {
    pub async fn init_all(&mut self) {
        for mut entry in self.plugins.iter_mut() {
            let plugin = entry.value_mut();
            if plugin.error.is_none() {
                if let Err(e) = plugin.storefront.init().await {
                    plugin.error = Some(format!("Failed to initialize plugin: {}", e));
                }
            }
        }
    }
}

impl Global for PluginManager {}

pub trait PluginManagerExt {
    fn plugin_manager(&self) -> &PluginManager;
}

impl PluginManagerExt for App {
    fn plugin_manager(&self) -> &PluginManager {
        self.global::<PluginManager>()
    }
}
