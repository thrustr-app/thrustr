use crate::{GameEntry, NewGame};
use anyhow::Result;

pub trait GameStorage: Send + Sync {
    fn insert(&self, game: &NewGame) -> Result<GameEntry>;
}
