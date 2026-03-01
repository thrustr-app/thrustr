use crate::metadata::Metadata;
use anyhow::Result;
use async_trait::async_trait;
use semver::Version;
use std::{path::Path, sync::Arc};

pub trait Plugin: Metadata + Send + Sync {
    fn version(&self) -> &Version;
    fn authors(&self) -> &[String];
}

#[async_trait]
pub trait PluginManager: Send + Sync {
    async fn load_plugins(&self, dir: &Path) -> Result<()>;
    async fn load_plugin(&self, path: &Path) -> Result<()>;
    fn plugins(&self) -> Vec<Arc<dyn Plugin>>;
    fn plugin(&self, name: &str) -> Option<Arc<dyn Plugin>>;
}
