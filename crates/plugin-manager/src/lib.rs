use crate::plugin::{Plugin, PluginBuilder, PluginManifest, PluginState};
use anyhow::Result;
use dashmap::DashMap;
use ports::{
    managers::{Plugin as PluginTrait, PluginManager as PluginManagerTrait, StorefrontManager},
    metadata::{Image, ImageFormat, Metadata},
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
    world: "storefront-provider-plugin",
    imports: { default: async },
    exports: { default: async }
});

#[derive(Clone)]
pub struct PluginManager {
    engine: Engine,
    linker: Arc<Linker<PluginState>>,
    plugins: Arc<DashMap<String, Arc<Plugin>>>,
    storage: Arc<dyn ExtensionStorage>,
    storefront_manager: Arc<dyn StorefrontManager>,
}

impl PluginManager {
    pub fn new(
        storage: Arc<dyn ExtensionStorage>,
        storefront_manager: Arc<dyn StorefrontManager>,
    ) -> Self {
        let mut config = Config::new();
        config.async_support(true);

        let engine = Engine::new(&config).expect("Failed to create Wasmtime engine");
        let mut linker = Linker::new(&engine);
        wasmtime_wasi::p2::add_to_linker_async(&mut linker).expect("Failed to add WASI to linker");
        StorefrontProviderPlugin::add_to_linker::<_, PluginState>(&mut linker, |state| state)
            .expect("Failed to add Storefront imports to linker");
        wasmtime_wasi_http::add_only_http_to_linker_async(&mut linker)
            .expect("Failed to add WASI HTTP to linker");

        Self {
            engine,
            linker: Arc::new(linker),
            plugins: Arc::new(DashMap::new()),
            storage,
            storefront_manager,
        }
    }
}

impl PluginManagerTrait for PluginManager {
    fn load_plugins(&self, dir: impl AsRef<Path>) -> Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("tp") {
                self.load_plugin(&path)?;
            }
        }
        Ok(())
    }

    fn load_plugin(&self, path: impl AsRef<Path>) -> Result<()> {
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

        let icon: Option<Image> = (0..archive.len()).find_map(|i| {
            let mut file = archive.by_index(i).ok()?;
            let name = file.name().to_lowercase();
            let path = Path::new(&name);
            if path.file_stem()?.to_str()? != "icon" {
                return None;
            }
            let format = ImageFormat::from_extension(path.extension()?.to_str()?)?;
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).ok()?;
            Some(Image { bytes, format })
        });

        let component = Component::from_binary(&self.engine, &wasm_bytes)?;
        let instance_pre = self.linker.instantiate_pre(&component)?;
        let storefront = StorefrontProviderPluginPre::new(instance_pre).ok();

        let plugin = Arc::new(
            PluginBuilder::new(manifest, self.engine.clone(), self.storage.clone())
                .icon(icon)
                .storefront_pre(storefront)
                .build(),
        );

        if let Some(s) = plugin.as_storefront_provider() {
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
