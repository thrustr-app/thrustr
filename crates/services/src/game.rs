use anyhow::Result;
use domain::{GameListEntry, storage::GameStorage};
use std::sync::Arc;

#[derive(Clone)]
pub struct GameService {
    game_storage: Arc<dyn GameStorage>,
}

impl GameService {
    pub fn new(game_storage: Arc<dyn GameStorage>) -> Self {
        Self { game_storage }
    }

    pub fn list(&self, offset: usize, limit: usize) -> Result<Vec<GameListEntry>> {
        self.game_storage.list(offset, limit)
    }
}
