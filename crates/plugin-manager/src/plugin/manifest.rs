use ports::component::{Config, LoginForm};
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
pub struct PluginManifest {
    pub plugin: PluginInfo,
    pub auth: Option<LoginForm>,
    pub config: Option<Config>,
}
