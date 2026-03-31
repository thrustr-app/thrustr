use crate::domain::game::{GameListEntry, GameRepository};
use anyhow::Result;
use std::sync::Arc;

#[derive(Clone)]
pub struct GameService {
    game_storage: Arc<dyn GameRepository>,
}

impl GameService {
    pub fn new(game_storage: Arc<dyn GameRepository>) -> Self {
        Self { game_storage }
    }

    pub fn list(&self, offset: usize, limit: usize) -> Result<Vec<GameListEntry>> {
        self.game_storage.list(offset, limit)
    }
}
