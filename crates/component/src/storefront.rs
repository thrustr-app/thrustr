use crate::{
    ComponentHandle,
    handle::{OperationError, Permit, Result},
};
use dashmap::DashMap;
use domain::{
    component::{Storefront, StorefrontOperation},
    game::{Game, GameId, GameVersion, GameVersionDetails, NewGame},
};
use event::Topic;
use runtime::clock::Moment;
use std::{sync::Arc, time::Duration};
use tracing::{info, warn};

const VERSION_DETAILS_TTL: Duration = Duration::from_hours(1);

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

    pub async fn list_game_versions(&self, game_id: GameId) -> Result<Vec<GameVersion>> {
        let permit = Permit::begin(&self.component, StorefrontOperation::ListVersions)?;

        let game = self.load_game(game_id).await?;
        let versions = match self.storefront.list_game_versions(game).await {
            Ok(versions) => versions,
            Err(error) => {
                permit.finish(Err(error.clone())).await?;
                return Err(error.into());
            }
        };
        permit.finish(Ok(())).await?;

        Ok(versions)
    }

    pub async fn game_version_details(
        &self,
        game_id: GameId,
        version_id: String,
    ) -> Result<GameVersionDetails> {
        let cache = self.component.version_details_cache();
        if let Some(details) = cache.get(game_id, &version_id) {
            return Ok(details);
        }

        let permit = Permit::begin(&self.component, StorefrontOperation::VersionDetails)?;

        let game = self.load_game(game_id).await?;
        let details = match self
            .storefront
            .game_version_details(game, version_id.clone())
            .await
        {
            Ok(details) => details,
            Err(error) => {
                permit.finish(Err(error.clone())).await?;
                return Err(error.into());
            }
        };
        permit.finish(Ok(())).await?;

        cache.insert(game_id, version_id, details.clone());
        Ok(details)
    }

    async fn load_game(&self, game_id: GameId) -> Result<Game> {
        let repository = self.component.context().game_repository.clone();
        let game = self
            .component
            .context()
            .tokio_handle
            .spawn_blocking(move || repository.get(game_id))
            .await
            .map_err(anyhow::Error::from)??
            .ok_or(OperationError::GameNotFound(game_id))?;

        if game.source.id != self.component.id() {
            return Err(OperationError::ForeignGame(game_id));
        }
        Ok(game)
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

#[derive(Default)]
pub(crate) struct VersionDetailsCache(DashMap<(GameId, String), (Moment, GameVersionDetails)>);

impl VersionDetailsCache {
    fn get(&self, game_id: GameId, version_id: &str) -> Option<GameVersionDetails> {
        let key = (game_id, version_id.to_owned());
        let now = Moment::now();
        self.0
            .remove_if(&key, |_, (expires_at, _)| expires_at.has_passed(now));
        self.0.get(&key).map(|entry| entry.1.clone())
    }

    fn insert(&self, game_id: GameId, version_id: String, details: GameVersionDetails) {
        if let Some(expires_at) = Moment::now().checked_add(VERSION_DETAILS_TTL) {
            self.0.insert((game_id, version_id), (expires_at, details));
        }
    }

    pub(crate) fn clear(&self) {
        self.0.clear();
    }
}
