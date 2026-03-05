use crate::plugin::{PluginBuilder, PluginManifest, PluginState};
use anyhow::Result;
use component_manager::ComponentManager;
use futures::TryStreamExt;
use ports::{
    component::{Image, ImageFormat},
    storage::ComponentStorage,
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
    component::{Component as WasmComponent, Linker, bindgen},
};
use zip::ZipArchive;

mod plugin;

bindgen!({
    path: "../thrustr-plugin/wit",
    world: "storefront-plugin",
    imports: { default: async },
    exports: { default: async }
});

#[derive(Clone)]
pub struct PluginManager {
    engine: Engine,
    linker: Arc<Linker<PluginState>>,
    storage: Arc<dyn ComponentStorage>,
    component_manager: Arc<ComponentManager>,
}

impl PluginManager {
    pub fn new(
        storage: Arc<dyn ComponentStorage>,
        component_manager: Arc<ComponentManager>,
    ) -> Self {
        let mut config = Config::new();
        config.async_support(true);

        let engine = Engine::new(&config).expect("Failed to create Wasmtime engine");
        let mut linker = Linker::new(&engine);
        wasmtime_wasi::p2::add_to_linker_async(&mut linker).expect("Failed to add WASI to linker");
        StorefrontPlugin::add_to_linker::<_, PluginState>(&mut linker, |state| state)
            .expect("Failed to add Storefront imports to linker");
        wasmtime_wasi_http::add_only_http_to_linker_async(&mut linker)
            .expect("Failed to add WASI HTTP to linker");

        Self {
            engine,
            linker: Arc::new(linker),
            storage,
            component_manager,
        }
    }

    pub async fn load_plugins(&self, dir: &Path) -> Result<()> {
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

    pub async fn load_plugin(&self, path: &Path) -> Result<()> {
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
            move || WasmComponent::from_binary(&engine, &wasm_bytes)
        })
        .await?;

        let instance_pre = self.linker.instantiate_pre(&component)?;
        let storefront = StorefrontPluginPre::new(instance_pre).ok();

        let plugin = Arc::new(
            PluginBuilder::new(manifest, self.engine.clone(), self.storage.clone())
                .icon(icon)
                .storefront_pre(storefront)
                .build(),
        );

        self.component_manager.register(plugin);

        event::emit("plugin");

        Ok(())
    }
}
