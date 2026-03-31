use crate::ComponentHandle;
use application::{
    component::{ComponentStorage, Status, storefront::Storefront},
    domain::game::GameRepository,
};
use std::sync::Arc;

#[derive(Clone)]
pub struct StorefrontHandle {
    storefront: Arc<dyn Storefront>,
    component_storage: Arc<dyn ComponentStorage>,
    game_storage: Arc<dyn GameRepository>,
}

impl StorefrontHandle {
    pub fn new(
        storefront: Arc<dyn Storefront>,
        component_storage: Arc<dyn ComponentStorage>,
        game_storage: Arc<dyn GameRepository>,
    ) -> Self {
        Self {
            storefront,
            component_storage,
            game_storage,
        }
    }

    pub fn component(&self) -> Option<ComponentHandle> {
        Arc::clone(&self.storefront)
            .component()
            .upgrade()
            .map(|component| {
                ComponentHandle::new(
                    component,
                    Arc::clone(&self.component_storage),
                    Arc::clone(&self.game_storage),
                )
            })
    }

    pub async fn fetch_games(&self) -> Result<(), String> {
        let component = self
            .component()
            .ok_or_else(|| "Component has been unregistered".to_string())?;

        if !component.status().is_active() {
            return Err("Storefront is not active.".into());
        }

        let new_games = self.storefront.list_games().await.map_err(|e| {
            let error = e.to_string();
            if let Some(component) = self.component() {
                component.set_status(Status::Error(e));
            }
            error
        })?;

        self.game_storage
            .insert_many(&new_games)
            .map_err(|err| err.to_string())?;

        Ok(())
    }
}
