use anyhow::Result;
use extism::{
    Manifest, PTR, Plugin as ExtismPlugin, PluginBuilder, UserData, Wasm, convert::Json, host_fn,
};
use models::{PluginManifest, SetPluginDataInput};
use semver::Version;
use std::{collections::HashMap, path::Path};

mod adapters;

#[derive(Debug)]
pub struct Plugin {
    manifest: PluginManifest,
    inner: ExtismPlugin,
}

impl Plugin {
    pub fn new(mut inner: ExtismPlugin) -> Result<Self> {
        let Json(manifest) = inner.call::<(), Json<PluginManifest>>("manifest", ())?;
        Ok(Self { manifest, inner })
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

pub struct PluginManager {
    plugins: HashMap<String, Plugin>,
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }

    pub fn load_plugins_from_dir(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let entries = std::fs::read_dir(path)?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("wasm") {
                self.load_plugin_from_file(&path)?;
            }
        }

        Ok(())
    }

    pub fn load_plugin_from_file(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let file = Wasm::file(path);
        self.load_plugin(file)
    }

    pub fn load_plugin_from_url(&mut self, url: &str) -> Result<()> {
        let file = Wasm::url(url);
        self.load_plugin(file)
    }

    pub fn list_plugins(&self) -> Vec<&str> {
        self.plugins.keys().map(String::as_str).collect()
    }

    fn load_plugin(&mut self, wasm: Wasm) -> Result<()> {
        let manifest = Manifest::new([wasm]);
        let extism_plugin = PluginBuilder::new(&manifest)
            .with_wasi(true)
            .with_function(
                "get_plugin_data",
                [PTR],
                [PTR],
                UserData::new(()),
                get_plugin_data,
            )
            .with_function(
                "set_plugin_data",
                [PTR],
                [PTR],
                UserData::new(()),
                set_plugin_data,
            )
            .build()?;

        let plugin = Plugin::new(extism_plugin)?;
        self.plugins.insert(plugin.id().to_string(), plugin);

        Ok(())
    }
}

host_fn!(get_plugin_data(key: String) -> Json<Map<String, Value>> {
    Ok(adapters::get_plugin_data(key))
});

host_fn!(set_plugin_data(input: Json<SetPluginDataInput>) -> bool {
    Ok(adapters::set_plugin_data(input))
});
