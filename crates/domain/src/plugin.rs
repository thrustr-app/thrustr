use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Serialize, Deserialize, Debug)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub authors: Vec<String>,
    pub version: Version,
    pub description: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SetPluginDataInput {
    pub id: String,
    pub data: Map<String, Value>,
}
