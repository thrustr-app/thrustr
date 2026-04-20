use crate::GameListItem;
use anyhow::Result;

pub trait GameQuery: Send + Sync {
    fn count(&self) -> Result<usize>;
    fn list(&self, offset: usize, limit: usize) -> Result<Vec<GameListItem>>;
}
