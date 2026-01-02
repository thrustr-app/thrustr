use anyhow::Result;
use async_trait::async_trait;
use domain::Storefront;

pub struct EpicGames;

#[async_trait]
impl Storefront for EpicGames {
    async fn init(&self) -> Result<()> {
        Ok(())
    }
}
