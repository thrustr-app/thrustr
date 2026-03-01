use crate::plugin::{Plugin, PluginBuilder, PluginManifest, PluginState};
use anyhow::Result;
use async_trait::async_trait;
use dashmap::DashMap;
use futures::TryStreamExt;
use ports::{
    managers::{Plugin as PluginTrait, PluginManager as PluginManagerTrait, StorefrontManager},
    metadata::{Image, ImageFormat, Metadata},
    storage::ExtensionStorage,
};
use std::{
    ffi::OsStr,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
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

#[async_trait]
impl PluginManagerTrait for PluginManager {
    async fn load_plugins(&self, dir: &Path) -> Result<()> {
        let mut read_dir = smol::fs::read_dir(dir).await?;
        let mut paths: Vec<PathBuf> = Vec::new();

        while let Some(entry) = read_dir.try_next().await? {
            let path = entry.path();
            if path.extension() == Some(OsStr::new("tp")) {
                paths.push(path);
            }
        }

        futures::stream::iter(paths.into_iter().map(Ok::<PathBuf, anyhow::Error>))
            .try_for_each_concurrent(None, |path| async move {
                self.load_plugin(path.as_path()).await
            })
            .await?;

        Ok(())
    }

    async fn load_plugin(&self, path: &Path) -> Result<()> {
        let path = path.to_owned();

        let (manifest, wasm_bytes, icon) = smol::unblock(move || {
            let file = File::open(&path)?;
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

            Ok::<_, anyhow::Error>((manifest, wasm_bytes, icon))
        })
        .await?;

        let component = smol::unblock({
            let engine = self.engine.clone();
            move || Component::from_binary(&engine, &wasm_bytes)
        })
        .await?;

        let instance_pre = self.linker.instantiate_pre(&component)?;
        let storefront = StorefrontProviderPluginPre::new(instance_pre).ok();

        let plugin = Arc::new(
            PluginBuilder::new(manifest, self.engine.clone(), self.storage.clone())
                .icon(icon)
                .storefront_pre(storefront)
                .build(),
        );

        if let Some(s) = plugin.as_storefront_provider() {
            self.storefront_manager
                .register_storefront_provider(s)
                .await;
        }

        self.plugins.insert(plugin.id().to_owned(), plugin);

        event::emit("plugin");

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
