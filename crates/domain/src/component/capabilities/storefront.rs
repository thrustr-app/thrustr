use crate::{
    component::Error,
    game::{Game, GameVersion, NewGame},
};
use async_trait::async_trait;

#[async_trait]
pub trait Storefront: Send + Sync {
    async fn list_games(&self) -> Result<Vec<NewGame>, Error>;
    async fn list_game_versions(&self, game: Game) -> Result<Vec<GameVersion>, Error>;
}
