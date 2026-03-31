use crate::{
    component::{Capability, Error},
    game::NewGame,
};
use async_trait::async_trait;

#[async_trait]
pub trait Storefront: Capability {
    async fn list_games(&self) -> Result<Vec<NewGame>, Error>;
}
