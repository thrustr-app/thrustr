use crate::plugin::{Plugin, PluginManifest, PluginState};
use anyhow::Result;
use dashmap::DashMap;
use gpui::{App, Global};
use ports::storage::ExtensionStorage;
use std::{
    fs::{self, File},
    io::Read,
    path::Path,
    sync::Arc,
};
use wasmtime::{
    Config, Engine,
    component::{Component, Linker, bindgen},
};
use zip::ZipArchive;

mod plugin;

bindgen!({
    path: "../thrustr-plugin/wit",
    world: "storefront",
    imports: { default: async },
    exports: { default: async }
});

pub fn init(cx: &mut App, storage: Arc<dyn ExtensionStorage>) {
    let mut config = Config::new();
    config.async_support(true).wasm_component_model(true);

    let engine = Engine::new(&config).expect("Failed to create Wasmtime engine");
    let mut linker = Linker::new(&engine);
    wasmtime_wasi::p2::add_to_linker_async(&mut linker).expect("Failed to add WASI to linker");
    Storefront::add_to_linker::<_, PluginState>(&mut linker, |state| state)
        .expect("Failed to add Storefront imports to linker");
    wasmtime_wasi_http::add_only_http_to_linker_async(&mut linker)
        .expect("Failed to add WASI HTTP to linker");

    cx.set_global(PluginManager {
        engine,
        linker: Arc::new(linker),
        plugins: Arc::new(DashMap::new()),
        storage,
    });
}

#[derive(Clone)]
pub struct PluginManager {
    engine: Engine,
    linker: Arc<Linker<PluginState>>,
    plugins: Arc<DashMap<String, Arc<Plugin>>>,
    storage: Arc<dyn ExtensionStorage>,
}

impl PluginManager {
    pub async fn load_plugins_from_dir(&self, dir: impl AsRef<Path>) -> Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("tp") {
                self.load_plugin_from_dir(&path).await?;
            }
        }
        Ok(())
    }

    pub async fn load_plugin_from_dir(&self, path: impl AsRef<Path>) -> Result<()> {
        let file = File::open(path)?;
        let mut archive = ZipArchive::new(file)?;

        let manifest: PluginManifest = {
            let mut manifest_file = archive.by_name("manifest.toml")?;
            let mut manifest_content = String::new();
            manifest_file.read_to_string(&mut manifest_content)?;
            toml::from_str(&manifest_content)?
        };

        let wasm_bytes: Vec<u8> = {
            let mut wasm_file = archive.by_name("plugin.wasm")?;
            let mut wasm_content = Vec::new();
            wasm_file.read_to_end(&mut wasm_content)?;
            wasm_content
        };

        let component = Component::from_binary(&self.engine, &wasm_bytes)?;

        let instance_pre = self.linker.instantiate_pre(&component)?;
        let storefront = StorefrontPre::new(instance_pre.clone()).ok();

        let mut plugin = Plugin::new(manifest, self.engine.clone(), self.storage.clone());
        plugin.set_storefront(storefront);

        let arc_plugin = Arc::new(plugin);

        self.plugins.insert(arc_plugin.id().to_owned(), arc_plugin);

        Ok(())
    }

    pub fn plugin(&self, name: &str) -> Option<Arc<Plugin>> {
        self.plugins.get(name).map(|p| Arc::clone(&p))
    }
}

impl Global for PluginManager {}

pub trait PluginManagerExt {
    fn plugin_manager(&self) -> PluginManager;
}

impl PluginManagerExt for App {
    fn plugin_manager(&self) -> PluginManager {
        self.global::<PluginManager>().clone()
    }
}
