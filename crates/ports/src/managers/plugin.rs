use anyhow::Result;
use semver::Version;
use std::{path::Path, sync::Arc};

pub trait Plugin: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn version(&self) -> &Version;
    fn description(&self) -> Option<&str>;
    fn authors(&self) -> &[String];
}

pub trait PluginManager: Send + Sync {
    fn load_plugins_from_dir(&self, dir: impl AsRef<Path>) -> Result<()>;
    fn load_plugin_from_dir(&self, path: impl AsRef<Path>) -> Result<()>;
    fn plugin(&self, name: &str) -> Option<Arc<dyn Plugin>>;
}
