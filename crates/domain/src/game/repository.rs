use crate::game::{GameListItem, NewGame};
use anyhow::Result;

pub trait GameRepository: Send + Sync {
    fn insert(&self, game: &NewGame) -> Result<()>;
    fn insert_many(&self, games: &[NewGame]) -> Result<()>;
    fn count(&self) -> Result<usize>;
    fn list(&self, offset: usize, limit: usize) -> Result<Vec<GameListItem>>;
}
