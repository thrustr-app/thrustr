use crate::{GameEntry, GameListEntry, NewGame};
use anyhow::Result;

pub trait GameStorage: Send + Sync {
    fn insert(&self, game: &NewGame) -> Result<GameEntry>;
    fn count(&self) -> Result<usize>;
    fn list(&self, offset: usize, limit: usize) -> Result<Vec<GameListEntry>>;
}
