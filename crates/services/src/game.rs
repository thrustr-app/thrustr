use anyhow::Result;
use domain::game::{GameListItem, GameRepository};
use std::sync::Arc;

#[derive(Clone)]
pub struct GameService {
    game_storage: Arc<dyn GameRepository>,
}

impl GameService {
    pub fn new(game_storage: Arc<dyn GameRepository>) -> Self {
        Self { game_storage }
    }

    pub fn list(&self, offset: usize, limit: usize) -> Result<Vec<GameListItem>> {
        self.game_storage.list(offset, limit)
    }
}
