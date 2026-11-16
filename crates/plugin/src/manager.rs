use crate::{
    plugin::{
        AllowedHosts, Plugin, PluginAuth, PluginCache, PluginConfig, PluginManifest, PluginRuntime,
        PluginScheduler, PluginState, PluginStorefront, add_fs_to_linker, http_client,
    },
    wit::{PluginHost, exports::thrustr::plugin::base},
};
use anyhow::{Result, anyhow, bail};
use component::ComponentLink;
use config::paths::{plugin_data_dir, plugins_cache_dir};
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
    Config as WasmtimeConfig, Engine,
    component::{Component as WasmComponent, Linker},
};
use wasmtime_wasi_http::p3::add_to_linker as add_http_to_linker;
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
        let mut config = WasmtimeConfig::new();
        config.wasm_component_model_async(true);
        config.wasm_component_model_map(true);
        config.consume_fuel(true);

        let engine = Engine::new(&config).expect("failed to create Wasmtime engine");
        let mut linker = Linker::new(&engine);

        add_fs_to_linker(&mut linker).expect("failed to add the plugin filesystem to linker");
        add_wasip3_to_linker(&mut linker).expect("failed to add WASIp3 to linker");
        add_http_to_linker(&mut linker).expect("failed to add WASIp3 HTTP to linker");

        PluginHost::add_to_linker::<_, PluginState>(&mut linker, |state| state)
            .expect("failed to add plugin imports to linker");

        Self {
            engine,
            linker: Arc::new(linker),
            storage,
            tokio_handle,
            http_client: http_client(),
        }
    }

    pub fn tokio_handle(&self) -> &TokioHandle {
        &self.tokio_handle
    }

    pub async fn load_plugin(&self, path: PathBuf, link: ComponentLink) -> Result<Plugin> {
        let engine = self.engine.clone();
        let (manifest, icon, component, data_dir) = self
            .tokio_handle
            .spawn_blocking(move || -> Result<_> {
                let (manifest, wasm_bytes, icon) = read_plugin_archive(&path)?;
                let id = manifest.plugin.id.as_str();
                let data_dir = plugin_data_dir().join(id);
                let component = load_component(&engine, id, &wasm_bytes)?;
                Ok((manifest, icon, component, data_dir))
            })
            .await??;

        let PluginManifest {
            plugin: info,
            auth: login_form,
            config: config_schema,
        } = manifest;

        let config_schema = config_schema.map(Arc::new);

        let runtime = Arc::new(PluginRuntime {
            id: info.id.clone(),
            allowed_hosts: Arc::new(AllowedHosts::new(&info.allowed_hosts)),
            engine: self.engine.clone(),
            pre: self.linker.instantiate_pre(&component)?,
            storage: self.storage.clone(),
            tokio_handle: self.tokio_handle.clone(),
            http_client: self.http_client.clone(),
            link,
            data_dir,
            cache: PluginCache::new(),
            config_schema: config_schema.clone(),
        });

        let base = runtime.export("base", base::GuestIndices::new)?.ok_or_else(|| {
            anyhow!("plugin does not export thrustr:plugin/base; add #[pdk::export] to its `impl pdk::Plugin`")
        })?;
        let auth = PluginAuth::resolve(&runtime, login_form)?;
        let config = PluginConfig::resolve(&runtime, config_schema)?;
        let storefront = PluginStorefront::resolve(&runtime)?;
        let scheduler = PluginScheduler::resolve(&runtime).await?;

        let plugin = Plugin {
            info,
            icon,
            runtime,
            base,
            auth: auth.map(|auth| Arc::new(auth) as _),
            config: config.map(|config| Arc::new(config) as _),
            storefront: storefront.map(|storefront| Arc::new(storefront) as _),
            scheduler: scheduler.map(|scheduler| Arc::new(scheduler) as _),
        };

        Ok(plugin)
    }
}

fn add_wasip3_to_linker(linker: &mut Linker<PluginState>) -> wasmtime::Result<()> {
    use wasmtime_wasi::p3;

    p3::cli::add_to_linker(linker)?;
    p3::clocks::add_to_linker(linker)?;
    p3::random::add_to_linker(linker)?;
    p3::sockets::add_to_linker(linker)
}

fn load_component(engine: &Engine, plugin_id: &str, wasm_bytes: &[u8]) -> Result<WasmComponent> {
    let hash = blake3::hash(wasm_bytes).to_hex();
    let cache_dir = plugins_cache_dir().join(plugin_id);
    let cache_path = cache_dir.join(format!("{hash}.cwasm"));

    // SAFETY: only this function writes the cache and Wasmtime rejects
    // artifacts built by another version or configuration.
    if let Ok(component) = unsafe { WasmComponent::deserialize_file(engine, &cache_path) } {
        return Ok(component);
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

fn read_plugin_archive(path: &Path) -> Result<(PluginManifest, Vec<u8>, Option<Image>)> {
    let mut archive = ZipArchive::new(File::open(path)?)?;

    let manifest = read_manifest(&mut archive)?;
    let wasm_bytes = read_entry(&mut archive, "plugin.wasm")?;
    let icon = read_icon(&mut archive)?;

    Ok((manifest, wasm_bytes, icon))
}

const MANIFEST_NAMES: [&str; 2] = ["manifest.yaml", "manifest.yml"];

fn read_manifest<R: Read + Seek>(archive: &mut ZipArchive<R>) -> Result<PluginManifest> {
    let mut names = MANIFEST_NAMES
        .into_iter()
        .filter(|name| archive.index_for_name(name).is_some());
    let name = match (names.next(), names.next()) {
        (Some(name), None) => name,
        (None, _) => bail!("plugin archive has no manifest.yaml or manifest.yml"),
        (Some(_), Some(_)) => bail!("plugin archive has both manifest.yaml and manifest.yml"),
    };

    Ok(serde_saphyr::from_slice(&read_entry(archive, name)?)?)
}

fn read_entry<R: Read + Seek>(archive: &mut ZipArchive<R>, name: &str) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    archive.by_name(name)?.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn read_icon<R: Read + Seek>(archive: &mut ZipArchive<R>) -> Result<Option<Image>> {
    let icon = archive.file_names().flatten().find_map(|name| {
        let (stem, ext) = name.rsplit_once('.')?;
        let format = ImageFormat::from_extension(ext)?;
        stem.eq_ignore_ascii_case("icon")
            .then(|| (name.into_owned(), format))
    });
    let Some((name, format)) = icon else {
        return Ok(None);
    };

    let bytes = read_entry(archive, &name)?;
    Ok(Some(Image { bytes, format }))
}
