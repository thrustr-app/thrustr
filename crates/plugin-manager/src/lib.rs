use anyhow::Result;
use domain::{PluginManifest, Storage};
use extism::{
    Manifest, PTR, Plugin as ExtismPlugin, PluginBuilder, UserData, Wasm, convert::Json, host_fn,
};
use semver::Version;
use serde_json::{Map, Value};
use std::{collections::HashMap, fs, path::Path, sync::Arc};
use tokio::runtime::Handle;

use crate::http::WorkerPool;

mod adapters;
mod http;

pub type SharedStorage = Arc<dyn Storage + Send + Sync>;

#[derive(Debug)]
pub struct Plugin {
    manifest: PluginManifest,
    inner: ExtismPlugin,
}

impl Plugin {
    pub fn new(inner: ExtismPlugin, manifest: PluginManifest) -> Result<Self> {
        Ok(Self { manifest, inner })
    }

    fn initialize(&mut self) -> Result<()> {
        self.inner.call::<(), ()>("initialize", ())?;
        Ok(())
    }

    pub fn id(&self) -> &str {
        &self.manifest.id
    }

    pub fn name(&self) -> &str {
        &self.manifest.name
    }

    pub fn version(&self) -> &Version {
        &self.manifest.version
    }

    pub fn description(&self) -> &str {
        &self.manifest.description
    }

    pub fn authors(&self) -> &[String] {
        &self.manifest.authors
    }
}

struct PluginContext {
    storage: SharedStorage,
    plugin_id: String,
    pool: Arc<WorkerPool>,
}

pub struct PluginManager {
    plugins: HashMap<String, Plugin>,
    storage: SharedStorage,
    pool: Arc<WorkerPool>,
}

impl PluginManager {
    pub fn new(tokio_handle: Handle, storage: SharedStorage) -> Self {
        Self {
            plugins: HashMap::new(),
            storage,
            pool: Arc::new(WorkerPool::new(tokio_handle, 4, 100)),
        }
    }

    pub fn load_plugins_from_dir(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let entries = std::fs::read_dir(path)?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                let wasm_path = path.join("plugin.wasm");
                let manifest_path = path.join("manifest.json");

                if wasm_path.exists() && manifest_path.exists() {
                    self.load_plugin_from_dir(&path)?;
                }
            }
        }

        Ok(())
    }

    pub fn load_plugin_from_dir(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let wasm_path = path.join("plugin.wasm");
        let manifest_path = path.join("manifest.json");

        let manifest_content = fs::read_to_string(&manifest_path)?;
        let manifest: PluginManifest = serde_json::from_str(&manifest_content)?;

        let wasm = Wasm::file(&wasm_path);
        self.load_plugin(wasm, manifest)
    }

    pub fn list_plugins(&self) -> Vec<&str> {
        self.plugins.keys().map(String::as_str).collect()
    }

    fn load_plugin(&mut self, wasm: Wasm, manifest: PluginManifest) -> Result<()> {
        let plugin_id = manifest.id.clone();

        let extism_manifest = Manifest::new([wasm]).with_allowed_host("*");
        let extism_plugin = PluginBuilder::new(&extism_manifest)
            .with_wasi(true)
            .with_function(
                "getPluginData",
                [],
                [PTR],
                UserData::new(PluginContext {
                    storage: Arc::clone(&self.storage),
                    plugin_id: plugin_id.clone(),
                    pool: Arc::clone(&self.pool),
                }),
                get_plugin_data,
            )
            .with_function(
                "setPluginData",
                [PTR],
                [PTR],
                UserData::new(PluginContext {
                    storage: Arc::clone(&self.storage),
                    plugin_id: plugin_id.clone(),
                    pool: Arc::clone(&self.pool),
                }),
                set_plugin_data,
            )
            .build()?;

        let mut plugin = Plugin::new(extism_plugin, manifest)?;

        plugin.initialize()?;

        self.plugins.insert(plugin.id().to_string(), plugin);
        Ok(())
    }
}

host_fn!(get_plugin_data(user_data: PluginContext;) -> Json<Option<Map<String, Value>>> {
    let context = user_data.get()?;
    let lock = context.lock().unwrap();
    let storage = Arc::clone(&lock.storage);
    let plugin_id = lock.plugin_id.clone();

    Ok(Json(adapters::get_plugin_data(&storage, plugin_id)))
});

host_fn!(set_plugin_data(user_data: PluginContext; data: Json<Map<String, Value>>) -> bool {
    let context = user_data.get()?;
    let lock = context.lock().unwrap();
    let storage = Arc::clone(&lock.storage);
    let plugin_id = lock.plugin_id.clone();
    let Json(data) = data;

    Ok(adapters::set_plugin_data(&storage, plugin_id, data))
});

host_fn!(make_request(user_data: PluginContext; url: String) -> String {
    let context = user_data.get()?;
    let lock = context.lock().unwrap();
    let pool = Arc::clone(&lock.pool);

    let request_id = pool.make_request(url).unwrap();

    Ok(request_id.to_string())
});

host_fn!(poll(user_data: PluginContext; id: String) -> Json<Status> {
    let context = user_data.get()?;
    let lock = context.lock().unwrap();
    let pool = Arc::clone(&lock.pool);

    let request_id = uuid::Uuid::parse_str(&id).unwrap();
    let status = pool.poll(request_id);

    Ok(Json(status))
});
