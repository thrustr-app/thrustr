use crate::component::ComponentHandle;
use domain::component::{Status, capabilities::Storefront};
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

    pub async fn fetch_games(&self) -> Result<(), String> {
        if !self.component.status().is_active() {
            return Err("Storefront is not active.".into());
        }
        let new_games = self.storefront.list_games().await.map_err(|e| {
            let error = e.to_string();
            self.component.set_status(Status::Error(e));
            error
        })?;
        self.component
            .context
            .game_storage
            .insert_many(&new_games)
            .map_err(|err| err.to_string())?;
        Ok(())
    }
}
