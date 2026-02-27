use crate::metadata::Metadata;
use anyhow::Result;
use semver::Version;
use std::{path::Path, sync::Arc};

pub trait Plugin: Metadata + Send + Sync {
    fn version(&self) -> &Version;
    fn authors(&self) -> &[String];
}

pub trait PluginManager: Send + Sync {
    fn load_plugins(&self, dir: impl AsRef<Path>) -> Result<()>;
    fn load_plugin(&self, path: impl AsRef<Path>) -> Result<()>;
    fn plugins(&self) -> Vec<Arc<dyn Plugin>>;
    fn plugin(&self, name: &str) -> Option<Arc<dyn Plugin>>;
}
