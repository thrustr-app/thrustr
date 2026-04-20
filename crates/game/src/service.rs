use crate::{GameListItem, GameQuery};
use anyhow::Result;
use std::sync::Arc;

#[derive(Clone)]
pub struct GameService {
    game_query: Arc<dyn GameQuery>,
}

impl GameService {
    pub fn new(game_query: Arc<dyn GameQuery>) -> Self {
        Self { game_query }
    }

    pub fn list(&self, offset: usize, limit: usize) -> Result<Vec<GameListItem>> {
        self.game_query.list(offset, limit)
    }
}
