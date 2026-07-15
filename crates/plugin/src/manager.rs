use crate::{
    plugin::{Plugin, PluginManifest, PluginState, http_client},
    wit::{StorefrontPlugin, StorefrontPluginPre},
};
use anyhow::Result;
use config::paths::plugins_cache_dir;
use domain::component::{ComponentStorage, Image, ImageFormat};
use reqwest::Client;
use runtime::TokioHandle;
use std::{
    fs::{self, File},
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
    tokio_handle: TokioHandle,
    http_client: Client,
}

impl PluginManager {
    pub fn new(storage: Arc<dyn ComponentStorage>, tokio_handle: TokioHandle) -> Self {
        let mut config = Config::new();
        config.wasm_component_model_async(true);
        config.consume_fuel(true);

        let engine = Engine::new(&config).expect("Failed to create Wasmtime engine");
        let mut linker = Linker::new(&engine);

        wasmtime_wasi::p2::add_to_linker_async(&mut linker)
            .expect("Failed to add WASIp2 to linker");

        wasmtime_wasi::p3::add_to_linker(&mut linker).expect("Failed to add WASIp3 to linker");

        wasmtime_wasi_http::p3::add_to_linker(&mut linker)
            .expect("Failed to add WASIp3 HTTP to linker");

        StorefrontPlugin::add_to_linker::<_, PluginState>(&mut linker, |state| state)
            .expect("Failed to add Storefront imports to linker");

        Self {
            engine,
            linker: Arc::new(linker),
            storage,
            tokio_handle,
            http_client: http_client(),
        }
    }

    pub(crate) fn tokio_handle(&self) -> &TokioHandle {
        &self.tokio_handle
    }

    pub async fn load_plugin(&self, path: PathBuf) -> Result<Plugin> {
        let (manifest, wasm_bytes, icon) = self
            .tokio_handle
            .spawn_blocking(move || read_plugin_archive(path))
            .await??;

        let component = self
            .tokio_handle
            .spawn_blocking({
                let engine = self.engine.clone();
                let plugin_id = manifest.plugin.id.clone();
                move || load_component(&engine, &plugin_id, &wasm_bytes)
            })
            .await??;

        let instance_pre = self.linker.instantiate_pre(&component)?;
        let storefront = StorefrontPluginPre::new(instance_pre).ok();

        let plugin = Plugin {
            allowed_hosts: manifest.plugin.allowed_hosts.as_slice().into(),
            manifest,
            icon,
            engine: self.engine.clone(),
            storage: self.storage.clone(),
            storefront_pre: storefront,
            tokio_handle: self.tokio_handle.clone(),
            http_client: self.http_client.clone(),
        };

        Ok(plugin)
    }
}

fn load_component(engine: &Engine, plugin_id: &str, wasm_bytes: &[u8]) -> Result<WasmComponent> {
    let hash = blake3::hash(wasm_bytes).to_hex();
    let cache_dir = plugins_cache_dir().join(plugin_id);
    let cache_path = cache_dir.join(format!("{hash}.cwasm"));

    if cache_path.exists() {
        match unsafe { WasmComponent::deserialize_file(engine, &cache_path) } {
            Ok(component) => return Ok(component),
            Err(_) => {
                let _ = fs::remove_file(&cache_path);
            }
        }
    }

    let component = WasmComponent::from_binary(engine, wasm_bytes)?;

    if let Ok(serialized) = component.serialize() {
        let _ = fs::remove_dir_all(&cache_dir);
        if fs::create_dir_all(&cache_dir).is_ok() {
            write_cache_atomic(&cache_path, &serialized);
        }
    }

    Ok(component)
}

fn write_cache_atomic(cache_path: &Path, bytes: &[u8]) {
    let tmp_path = cache_path.with_extension(format!("cwasm.{}.tmp", std::process::id()));
    if fs::write(&tmp_path, bytes).is_ok() && fs::rename(&tmp_path, cache_path).is_err() {
        let _ = fs::remove_file(&tmp_path);
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
