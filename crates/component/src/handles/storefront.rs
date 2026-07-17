use crate::ComponentHandle;
use domain::component::{StatusEvent, capabilities::Storefront};
use std::sync::Arc;

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
        if !self.component.status().is_active() {
            return Err("Storefront is not active.".into());
        }

        let new_games = self.storefront.list_games().await.map_err(|e| {
            let error = e.to_string();
            self.component.transition(StatusEvent::OperationFailed(e));
            error
        })?;

        if new_games.is_empty() {
            return Ok(());
        }

        let repository = self.component.context.game_repository.clone();
        let inserted = self
            .component
            .context
            .tokio_handle
            .spawn_blocking(move || repository.insert_many(&new_games))
            .await
            .map_err(|err| err.to_string())?
            .map_err(|err| err.to_string())?;

        if inserted == 0 {
            return Ok(());
        }

        event::emit("games");

        self.component.context.artwork_service.trigger_backfill();

        Ok(())
    }
}
