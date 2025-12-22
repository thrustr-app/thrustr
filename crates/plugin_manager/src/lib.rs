use crate::exports::thrustr::plugin::plugin_manifest;
use anyhow::Result;
use std::{collections::HashMap, fs, path::Path};
use wasmtime::{
    Config, Engine, Store,
    component::{Component, Linker, ResourceTable, bindgen},
};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView, p2::add_to_linker_sync};

bindgen!("plugin" in "../../wit");

struct PluginState {
    ctx: WasiCtx,
    table: ResourceTable,
}

impl WasiView for PluginState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.ctx,
            table: &mut self.table,
        }
    }
}

/// A loaded plugin instance, with WASI context and WIT bindings.
pub struct LoadedPlugin {
    pub id: String,
    pub name: String,
    store: Store<PluginState>,
    bindings: Plugin,
}

impl LoadedPlugin {
    /// Returns the ID of the plugin.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the name of the plugin.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Retrieves the manifest of the plugin.
    pub fn get_manifest(&mut self) -> Result<plugin_manifest::Manifest> {
        let manifest = self
            .bindings
            .thrustr_plugin_plugin_manifest()
            .call_get(&mut self.store)?;
        Ok(manifest)
    }

    pub fn init(&mut self) -> Result<()> {
        let res = self
            .bindings
            .thrustr_plugin_storefront()
            .call_init(&mut self.store)?;
        println!("{:?}", res);
        Ok(())
    }
}

pub struct PluginManager {
    engine: Engine,
    plugins: HashMap<String, LoadedPlugin>,
    linker: Linker<PluginState>,
}

impl PluginManager {
    /// Creates a new PluginManager with WASI support.
    pub fn new() -> Result<Self> {
        let mut config = Config::new();
        config.wasm_component_model(true);

        let engine = Engine::new(&config)?;

        let mut linker = Linker::new(&engine);
        add_to_linker_sync(&mut linker)?;

        Ok(Self {
            engine,
            plugins: HashMap::new(),
            linker,
        })
    }

    /// Loads all `.wasm` plugins from the specified directory.
    pub fn load_plugins_from_dir(&mut self, dir: impl AsRef<Path>) -> Result<()> {
        for entry in fs::read_dir(dir)? {
            let path = entry?.path();

            if path.extension().and_then(|s| s.to_str()) != Some("wasm") {
                continue;
            }

            let plugin = self.load_plugin(&path)?;
            let id = plugin.id.clone();
            self.plugins.insert(id, plugin);
        }
        Ok(())
    }

    /// Loads a single plugin from the specified path.
    pub fn load_plugin(&self, path: &Path) -> Result<LoadedPlugin> {
        let ctx = WasiCtxBuilder::new().build();
        let table = ResourceTable::new();
        let mut store = Store::new(&self.engine, PluginState { ctx, table });

        let component = Component::from_file(&self.engine, path)?;
        let bindings = Plugin::instantiate(&mut store, &component, &self.linker)?;

        let mut plugin = LoadedPlugin {
            id: String::new(),
            name: String::new(),
            store,
            bindings,
        };

        let manifest = plugin.get_manifest()?;
        plugin.id = manifest.id.clone();
        plugin.name = manifest.name.clone();

        Ok(plugin)
    }

    /// Lists all loaded plugin IDs.
    pub fn list_plugins(&self) -> Vec<&str> {
        self.plugins.keys().map(|s| s.as_str()).collect()
    }

    /// Retrieves a loaded plugin by its ID.
    pub fn get_plugin_mut(&mut self, id: &str) -> Option<&mut LoadedPlugin> {
        self.plugins.get_mut(id)
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new().unwrap_or_else(|e| {
            panic!("Failed to create PluginManager: {e}");
        })
    }
}
