use crate::manager::PluginManager;
use anyhow::{Result, anyhow};
use component::ComponentRegistry;
use domain::component::ComponentStorage;
use futures::StreamExt;
use runtime::TokioHandle;
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::Arc,
};
use tracing::error;

#[derive(Clone)]
pub struct PluginService {
    manager: PluginManager,
    component_registry: ComponentRegistry,
}

impl PluginService {
    pub fn new(
        storage: Arc<dyn ComponentStorage>,
        component_registry: ComponentRegistry,
        tokio_handle: TokioHandle,
    ) -> Self {
        Self {
            manager: PluginManager::new(storage, tokio_handle),
            component_registry,
        }
    }

    pub async fn load_and_init(&self, dir: &Path) -> Result<()> {
        let this = self.clone();
        let dir = dir.to_path_buf();
        self.manager
            .tokio_handle()
            .spawn(async move { this.load_dir(dir).await })
            .await?
    }

    async fn load_dir(&self, dir: PathBuf) -> Result<()> {
        let mut read_dir = tokio::fs::read_dir(&dir).await?;
        let mut paths: Vec<PathBuf> = Vec::new();

        while let Some(entry) = read_dir.next_entry().await? {
            let path = entry.path();
            if path.extension() == Some(OsStr::new("tp")) {
                paths.push(path);
            }
        }

        futures::stream::iter(paths)
            .for_each_concurrent(None, |path| async move {
                if let Err(err) = self.load_and_init_plugin(&path).await {
                    error!("failed to load plugin {}: {err:#}", path.display());
                }
            })
            .await;

        Ok(())
    }

    async fn load_and_init_plugin(&self, path: &Path) -> Result<()> {
        let plugin = self.manager.load_plugin(path.to_path_buf()).await?;
        event::emit("plugin");

        let component = self.component_registry.register(Arc::new(plugin))?;
        component.init().await.map_err(|err| anyhow!(err))?;

        Ok(())
    }
}
