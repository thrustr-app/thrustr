use anyhow::Result;
use domain::game::{Game, GameId, GameListItem, GameRepository};
use std::sync::Arc;

#[derive(Clone)]
pub struct GameService {
    game_repo: Arc<dyn GameRepository>,
}

impl GameService {
    pub fn new(game_repo: Arc<dyn GameRepository>) -> Self {
        Self { game_repo }
    }

    pub fn get(&self, id: GameId) -> Result<Option<Game>> {
        self.game_repo.get(id)
    }

    pub fn list(&self, offset: usize, limit: usize) -> Result<Vec<GameListItem>> {
        self.game_repo.list(offset, limit)
    }
}
