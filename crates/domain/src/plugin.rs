use semver::Version;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct PluginManifest {
    pub plugin: PluginInfo,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub authors: Vec<String>,
    pub version: Version,
    pub description: Option<String>,
}
