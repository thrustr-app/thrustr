use crate::{component::Error, game::NewGame};
use async_trait::async_trait;

#[async_trait]
pub trait Storefront: Send + Sync {
    async fn list_games(&self) -> Result<Vec<NewGame>, Error>;
}
