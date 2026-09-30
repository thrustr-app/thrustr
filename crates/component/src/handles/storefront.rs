use super::error::Result;
use super::permit::Permit;
use crate::ComponentHandle;
use domain::{
    component::{Storefront, StorefrontOperation},
    game::NewGame,
};
use event::Topic;
use std::sync::Arc;
use tracing::{info, warn};

#[derive(Clone)]
pub struct StorefrontHandle {
    storefront: Arc<dyn Storefront>,
    component: ComponentHandle,
}

impl StorefrontHandle {
    pub(crate) fn new(storefront: Arc<dyn Storefront>, component: ComponentHandle) -> Self {
        Self {
            storefront,
            component,
        }
    }

    pub fn component(&self) -> &ComponentHandle {
        &self.component
    }

    pub async fn sync_games(&self) -> Result<()> {
        let permit = Permit::begin(&self.component, StorefrontOperation::Sync)?;

        let games = match self.storefront.list_games().await {
            Ok(games) => games,
            Err(error) => {
                permit.finish(Err(error.clone())).await?;
                return Err(error.into());
            }
        };

        let listed = games.len();
        let inserted = match games.is_empty() {
            true => 0,
            false => self.store_games(games).await?,
        };
        permit.finish(Ok(())).await?;

        info!(
            component = self.component.id(),
            listed, inserted, "games synced"
        );

        if inserted == 0 {
            return Ok(());
        }

        event::emit(Topic::Games);

        self.component.context().artwork_service.trigger_backfill();

        Ok(())
    }

    async fn store_games(&self, games: Vec<NewGame>) -> Result<usize> {
        let repository = self.component.context().game_repository.clone();
        let inserted = self
            .component
            .context()
            .tokio_handle
            .spawn_blocking(move || repository.insert_many(&games))
            .await
            .map_err(anyhow::Error::from)?
            .inspect_err(
                |e| warn!(component = self.component.id(), error = %e, "storing games failed"),
            )?;

        Ok(inserted)
    }
}
