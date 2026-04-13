use crate::component::{ComponentHandle, RegistryContext};
use domain::component::{Status, capabilities::Storefront};
use std::sync::Arc;

#[derive(Clone)]
pub struct StorefrontHandle {
    storefront: Arc<dyn Storefront>,
    context: RegistryContext,
}

impl StorefrontHandle {
    pub fn new(storefront: Arc<dyn Storefront>, context: RegistryContext) -> Self {
        Self {
            storefront,
            context,
        }
    }

    pub fn component(&self) -> Option<ComponentHandle> {
        Arc::clone(&self.storefront)
            .component()
            .upgrade()
            .map(|component| ComponentHandle::new(component, self.context.clone()))
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

        self.context
            .game_storage
            .insert_many(&new_games)
            .map_err(|err| err.to_string())?;

        Ok(())
    }
}
