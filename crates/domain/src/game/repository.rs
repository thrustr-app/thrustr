use crate::game::NewGame;
use anyhow::Result;

pub trait GameRepository: Send + Sync {
    fn insert(&self, game: &NewGame) -> Result<()>;
    fn insert_many(&self, games: &[NewGame]) -> Result<()>;
}
