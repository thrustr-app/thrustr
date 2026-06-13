use anyhow::Result;
use domain::game::{GameListItem, GameRepository};
use std::sync::Arc;

#[derive(Clone)]
pub struct GameService {
    game_repo: Arc<dyn GameRepository>,
}

impl GameService {
    pub fn new(game_repo: Arc<dyn GameRepository>) -> Self {
        Self { game_repo }
    }

    pub fn list(&self, offset: usize, limit: usize) -> Result<Vec<GameListItem>> {
        self.game_repo.list(offset, limit)
    }
}
