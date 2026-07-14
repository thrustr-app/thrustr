use domain::component::{ComponentConfig, LoginForm};
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
    #[serde(default, rename = "allowed-hosts")]
    pub allowed_hosts: Vec<String>,
}

#[derive(Deserialize, Debug)]
pub struct PluginManifest {
    pub plugin: PluginInfo,
    pub auth: Option<LoginForm>,
    pub config: Option<ComponentConfig>,
}
