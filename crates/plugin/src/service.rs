use crate::manager::PluginManager;
use anyhow::{Result, anyhow};
use application::component::ComponentRegistry;
use domain::component::ComponentStorage;
use futures::TryStreamExt;
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone)]
pub struct PluginService {
    manager: PluginManager,
    component_registry: ComponentRegistry,
}

impl PluginService {
    pub fn new(storage: Arc<dyn ComponentStorage>, component_registry: ComponentRegistry) -> Self {
        Self {
            manager: PluginManager::new(storage),
            component_registry,
        }
    }

    pub async fn load_and_init(&self, dir: &Path) -> Result<()> {
        let mut read_dir = smol::fs::read_dir(dir).await?;
        let mut paths: Vec<PathBuf> = Vec::new();

        while let Some(entry) = read_dir.try_next().await? {
            let path = entry.path();
            if path.extension() == Some(OsStr::new("tp")) {
                paths.push(path);
            }
        }

        futures::stream::iter(paths.into_iter().map(Ok::<PathBuf, anyhow::Error>))
            .try_for_each_concurrent(None, |path| async move {
                {
                    let plugin = self.manager.load_plugin(path).await?;
                    event::emit("plugin");

                    let handle = self.component_registry.register(Arc::new(plugin));
                    handle.init().await.map_err(|err| anyhow!(err))?;

                    Ok(())
                }
            })
            .await?;

        Ok(())
    }
}
