use crate::{
    component::{Error, Operation},
    download::DownloadReporter,
    game::{Game, GameVersion, GameVersionDetails, NewGame},
};
use async_trait::async_trait;
use std::{path::PathBuf, sync::Arc};
use strum::Display;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Display)]
#[strum(serialize_all = "lowercase")]
pub enum StorefrontOperation {
    Sync,
    #[strum(to_string = "list versions")]
    ListVersions,
    #[strum(to_string = "version details")]
    VersionDetails,
    Download,
}

impl From<StorefrontOperation> for Operation {
    fn from(operation: StorefrontOperation) -> Self {
        Self::Storefront(operation)
    }
}

#[async_trait]
pub trait Storefront: Send + Sync {
    async fn games(&self) -> Result<Vec<NewGame>, Error>;

    async fn game_versions(&self, game: Game, path: Vec<String>)
    -> Result<Vec<GameVersion>, Error>;

    async fn game_version_details(
        &self,
        game: Game,
        path: Vec<String>,
    ) -> Result<GameVersionDetails, Error>;

    async fn download(
        &self,
        game: Game,
        path: Vec<String>,
        target_dir: PathBuf,
        reporter: Arc<dyn DownloadReporter>,
    ) -> Result<(), Error>;
}
