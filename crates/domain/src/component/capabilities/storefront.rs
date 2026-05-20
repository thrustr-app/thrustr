use crate::{
    component::Error,
    game::{Game, GameVersion, NewGame},
};
use async_trait::async_trait;

#[async_trait]
pub trait Storefront: Send + Sync {
    async fn get_games(&self) -> Result<Vec<NewGame>, Error>;
    async fn get_game_versions(&self, game: Game) -> Result<Vec<GameVersion>, Error>;
}
