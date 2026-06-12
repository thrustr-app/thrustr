use crate::game::{Game, GameListItem, NewGame};
use anyhow::Result;

pub trait GameRepository: Send + Sync {
    fn insert(&self, game: &NewGame) -> Result<Option<Game>>;

    fn insert_many(&self, games: &[NewGame]) -> Result<Vec<Game>>;

    fn count(&self) -> Result<usize>;

    fn list(&self, offset: usize, limit: usize) -> Result<Vec<GameListItem>>;
}
