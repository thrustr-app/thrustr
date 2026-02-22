use crate::plugin::{Plugin, PluginManifest, PluginState};
use anyhow::Result;
use dashmap::DashMap;
use gpui::{App, Global};
use ports::{
    managers::{Plugin as PluginTrait, PluginManager as PluginManagerTrait, StorefrontManager},
    storage::ExtensionStorage,
};
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

pub fn init(
    cx: &mut App,
    storage: Arc<dyn ExtensionStorage>,
    storefront_manager: Arc<dyn StorefrontManager>,
) {
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
        storefront_manager,
    });
}

#[derive(Clone)]
pub struct PluginManager {
    engine: Engine,
    linker: Arc<Linker<PluginState>>,
    plugins: Arc<DashMap<String, Arc<Plugin>>>,
    storage: Arc<dyn ExtensionStorage>,
    storefront_manager: Arc<dyn StorefrontManager>,
}

impl PluginManagerTrait for PluginManager {
    fn load_plugins_from_dir(&self, dir: impl AsRef<Path>) -> Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("tp") {
                self.load_plugin_from_dir(&path)?;
            }
        }
        Ok(())
    }

    fn load_plugin_from_dir(&self, path: impl AsRef<Path>) -> Result<()> {
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

        let plugin = {
            let mut plugin = Plugin::new(manifest, self.engine.clone(), self.storage.clone());
            plugin.set_storefront(storefront);
            Arc::new(plugin)
        };

        if let Some(s) = plugin.as_storefront() {
            self.storefront_manager.register_storefront_provider(s);
        }

        self.plugins.insert(plugin.id().to_owned(), plugin);

        Ok(())
    }

    fn plugins(&self) -> Vec<Arc<dyn PluginTrait>> {
        self.plugins
            .iter()
            .map(|p| p.value().clone() as Arc<dyn PluginTrait>)
            .collect()
    }

    fn plugin(&self, name: &str) -> Option<Arc<dyn PluginTrait>> {
        self.plugins
            .get(name)
            .map(|p| p.value().clone() as Arc<dyn PluginTrait>)
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
