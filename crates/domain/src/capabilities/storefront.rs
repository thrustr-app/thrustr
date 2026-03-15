use crate::{NewGame, capabilities::Capability, component::Error};
use async_trait::async_trait;

#[async_trait]
pub trait Storefront: Capability {
    async fn list_games(&self) -> Result<Vec<NewGame>, Error>;
}
