use crate::exports::thrustr::plugin::{
    plugin_manifest::{self, Manifest as PluginManifest},
    storefront,
};
use wit_bindgen::generate;

generate!("plugin" in "../../wit");

struct Plugin;

impl plugin_manifest::Guest for Plugin {
    fn get() -> PluginManifest {
        PluginManifest {
            id: "epic-games".to_string(),
            author: vec!["Jorge Pardo".to_string()],
            name: "Epic Games".to_string(),
            version: (1, 0, 0),
            description: "Plugin for Epic Games integration.".to_string(),
        }
    }
}

impl storefront::Guest for Plugin {
    fn init() -> Result<(), String> {
        println!("Hello world!");
        Ok(())
    }

    fn get_games() -> Result<u8, String> {
        Ok(42)
    }
}

export!(Plugin);
