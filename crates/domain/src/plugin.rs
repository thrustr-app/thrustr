use semver::Version;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub authors: Vec<String>,
    pub version: Version,
    pub description: String,
}
