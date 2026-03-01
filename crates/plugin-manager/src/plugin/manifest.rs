use semver::Version;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub authors: Vec<String>,
    pub version: Version,
    pub description: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
pub enum PluginConfigItem {
    Section {
        name: String,
        items: Vec<PluginConfigItem>,
    },
    String {
        id: String,
        label: String,
    },
}

#[derive(Deserialize, Debug)]
pub struct PluginConfig {
    #[serde(rename = "item")]
    pub items: Vec<PluginConfigItem>,
}

#[derive(Deserialize, Debug)]
pub struct PluginManifest {
    pub plugin: PluginInfo,
    pub config: Option<PluginConfig>,
}
