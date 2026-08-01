use crate::{Claim, ComponentHandle, Operation};
use domain::{
    component::{StatusEvent, capabilities::Storefront},
    game::NewGame,
};
use std::sync::Arc;
use strum::Display;
use tracing::{debug, info, warn};

/// An operation that runs against a component's storefront.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Display)]
#[strum(serialize_all = "lowercase")]
pub enum StorefrontOperation {
    #[strum(to_string = "game sync")]
    Sync,
}

impl StorefrontOperation {
    pub(super) fn is_exclusive(self) -> bool {
        match self {
            Self::Sync => false,
        }
    }
}

impl From<StorefrontOperation> for Operation {
    fn from(operation: StorefrontOperation) -> Self {
        Self::Storefront(operation)
    }
}

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

    /// Claims the component for a storefront operation.
    pub fn begin(&self, operation: StorefrontOperation) -> Option<Claim> {
        self.component.begin(operation.into())
    }

    pub async fn sync_games(&self, claim: &mut Claim) -> Result<(), String> {
        claim.transition(StorefrontOperation::Sync.into())?;

        let status = self.component.status();
        if !status.is_active() {
            debug!(
                component = self.component.id(),
                %status,
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
