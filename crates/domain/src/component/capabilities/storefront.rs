use crate::{
    component::{Error, Operation},
    game::{Game, GameVersion, NewGame},
};
use async_trait::async_trait;
use strum::Display;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Display)]
#[strum(serialize_all = "lowercase")]
pub enum StorefrontOperation {
    Sync,
}

impl From<StorefrontOperation> for Operation {
    fn from(operation: StorefrontOperation) -> Self {
        Self::Storefront(operation)
    }
}

#[async_trait]
pub trait Storefront: Send + Sync {
    async fn list_games(&self) -> Result<Vec<NewGame>, Error>;
    async fn list_game_versions(&self, game: Game) -> Result<Vec<GameVersion>, Error>;
}
