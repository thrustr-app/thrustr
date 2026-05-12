use crate::game::{Game, NewGame};
use anyhow::Result;

pub trait GameRepository: Send + Sync {
    fn insert(&self, game: &NewGame) -> Result<Option<Game>>;
    fn insert_many(&self, games: &[NewGame]) -> Result<Vec<Game>>;
}
