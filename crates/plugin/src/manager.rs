use crate::{
    plugin::{Plugin, PluginManifest, PluginState},
    wit::{StorefrontPlugin, StorefrontPluginPre},
};
use anyhow::Result;
use application::component::ComponentRegistry;
use domain::component::{ComponentStorage, Image, ImageFormat};
use futures::TryStreamExt;
use std::{
    ffi::OsStr,
    fs::File,
    io::{Read, Seek},
    path::{Path, PathBuf},
    sync::Arc,
};
use wasmtime::{
    Config, Engine,
    component::{Component as WasmComponent, Linker},
};
use zip::ZipArchive;

#[derive(Clone)]
pub struct PluginManager {
    engine: Engine,
    linker: Arc<Linker<PluginState>>,
    storage: Arc<dyn ComponentStorage>,
    component_registry: ComponentRegistry,
}

impl PluginManager {
    pub fn new(storage: Arc<dyn ComponentStorage>, component_registry: ComponentRegistry) -> Self {
        let config = Config::new();
        let engine = Engine::new(&config).expect("Failed to create Wasmtime engine");
        let mut linker = Linker::new(&engine);

        wasmtime_wasi::p2::add_to_linker_async(&mut linker).expect("Failed to add WASI to linker");

        wasmtime_wasi_http::p2::add_only_http_to_linker_async(&mut linker)
            .expect("Failed to add WASI HTTP to linker");

        StorefrontPlugin::add_to_linker::<_, PluginState>(&mut linker, |state| state)
            .expect("Failed to add Storefront imports to linker");

        Self {
            engine,
            linker: Arc::new(linker),
            storage,
            component_registry,
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
            .try_for_each_concurrent(None, |path| async move { self.load_plugin(path).await })
            .await?;

        Ok(())
    }

    pub async fn load_plugin(&self, path: PathBuf) -> Result<()> {
        let (manifest, wasm_bytes, icon) = smol::unblock(move || read_plugin_archive(path)).await?;

        let component = smol::unblock({
            let engine = self.engine.clone();
            move || WasmComponent::from_binary(&engine, &wasm_bytes)
        })
        .await?;

        let instance_pre = self.linker.instantiate_pre(&component)?;
        let storefront = StorefrontPluginPre::new(instance_pre).ok();

        let plugin = Plugin {
            manifest,
            icon,
            engine: self.engine.clone(),
            storage: self.storage.clone(),
            storefront_pre: storefront,
        };

        self.component_registry.register(Arc::new(plugin));

        event::emit("plugin");
        Ok(())
    }
}

fn read_plugin_archive(path: PathBuf) -> Result<(PluginManifest, Vec<u8>, Option<Image>)> {
    let file = File::open(&path)?;
    let mut archive = ZipArchive::new(file)?;

    let manifest = read_manifest(&mut archive)?;
    let wasm_bytes = read_wasm(&mut archive)?;
    let icon = read_icon(&mut archive)?;

    Ok((manifest, wasm_bytes, icon))
}

fn read_manifest<R: Read + Seek>(archive: &mut ZipArchive<R>) -> Result<PluginManifest> {
    let mut file = archive.by_name("manifest.toml")?;
    let mut content = String::new();
    file.read_to_string(&mut content)?;
    Ok(toml::from_str(&content)?)
}

fn read_wasm<R: Read + Seek>(archive: &mut ZipArchive<R>) -> Result<Vec<u8>> {
    let mut file = archive.by_name("plugin.wasm")?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn read_icon<R: Read + Seek>(archive: &mut ZipArchive<R>) -> Result<Option<Image>> {
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;

        let path = match file.enclosed_name() {
            Some(p) => p,
            None => continue,
        };

        let stem = path.file_stem().and_then(|s| s.to_str());
        if !stem.is_some_and(|s| s.eq_ignore_ascii_case("icon")) {
            continue;
        }

        let ext = match path.extension().and_then(|e| e.to_str()) {
            Some(ext) => ext,
            None => continue,
        };

        let format = match ImageFormat::from_extension(ext) {
            Some(f) => f,
            None => continue,
        };

        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;

        return Ok(Some(Image { bytes, format }));
    }

    Ok(None)
}
