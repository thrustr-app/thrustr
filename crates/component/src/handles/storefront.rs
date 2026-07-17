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

    pub async fn get_games(&self) -> Result<(), String> {
        if !self.component.status().is_active() {
            return Err("Storefront is not active.".into());
        }

        let new_games = self.storefront.get_games().await.map_err(|e| {
            let error = e.to_string();
            self.component.transition(StatusEvent::SyncFailed(e));
            error
        })?;

        if new_games.is_empty() {
            return Ok(());
        }

        let inserted = self
            .component
            .context
            .game_repository
            .insert_many(&new_games)
            .map_err(|err| err.to_string())?;

        if inserted == 0 {
            return Ok(());
        }

        event::emit("games");

        self.component.context.artwork_service.trigger_backfill();

        Ok(())
    }
}
