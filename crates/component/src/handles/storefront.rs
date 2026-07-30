use crate::ComponentHandle;
use domain::{
    component::{StatusEvent, capabilities::Storefront},
    game::NewGame,
};
use std::sync::Arc;
use tracing::{debug, info, warn};

#[derive(Clone)]
pub struct StorefrontHandle {
    storefront: Arc<dyn Storefront>,
    component: ComponentHandle,
}

impl StorefrontHandle {
    pub fn new(storefront: Arc<dyn Storefront>, component: ComponentHandle) -> Self {
        Self {
            storefront,
            component,
        }
    }

    pub fn component(&self) -> &ComponentHandle {
        &self.component
    }

    pub async fn sync_games(&self) -> Result<(), String> {
        let status = self.component.status();
        if !status.is_active() {
            debug!(
                component = self.component.id(),
                ?status,
                "game sync rejected"
            );
            return Err("Storefront is not active.".into());
        }

        let new_games = self.storefront.list_games().await.map_err(|e| {
            let error = e.to_string();
            warn!(component = self.component.id(), error = %e, "listing games failed");
            self.component.transition(StatusEvent::OperationFailed(e));
            error
        })?;

        let listed = new_games.len();
        let inserted = match new_games.is_empty() {
            true => 0,
            false => self.store_games(new_games).await?,
        };

        info!(
            component = self.component.id(),
            listed, inserted, "games synced"
        );

        if inserted == 0 {
            return Ok(());
        }

        event::emit("games");

        self.component.context.artwork_service.trigger_backfill();

        Ok(())
    }

    async fn store_games(&self, games: Vec<NewGame>) -> Result<usize, String> {
        let repository = self.component.context.game_repository.clone();
        self.component
            .context
            .tokio_handle
            .spawn_blocking(move || repository.insert_many(&games))
            .await
            .map_err(|err| err.to_string())?
            .map_err(|err| {
                warn!(component = self.component.id(), error = %err, "storing games failed");
                err.to_string()
            })
    }
}
