use anyhow::Result;
use extism::{Manifest, Plugin, Wasm};
use std::{collections::HashMap, path::Path};

pub struct PluginManager {
    plugins: HashMap<String, Plugin>,
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }

    pub fn load_plugins_from_dir(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let entries = std::fs::read_dir(path)?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("wasm") {
                self.load_plugin_from_file(&path)?;
            }
        }

        Ok(())
    }

    pub fn load_plugin_from_file(&self, path: impl AsRef<Path>) -> Result<()> {
        let file = Wasm::file(path);
        self.load_plugin(file)
    }

    pub fn load_plugin_from_url(&self, url: &str) -> Result<()> {
        let file = Wasm::url(url);
        self.load_plugin(file)
    }

    fn load_plugin(&self, wasm: Wasm) -> Result<()> {
        let manifest = Manifest::new([wasm]);
        let mut plugin = Plugin::new(&manifest, [], true)?;

        let res = plugin.call::<(), &str>("manifest", ())?;
        println!("Loaded plugin manifest: {:?}", res);

        Ok(())
    }
}
