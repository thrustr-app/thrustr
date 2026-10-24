use crate::{
    ComponentHandle,
    handle::{OperationError, Permit, Result},
};
use dashmap::DashMap;
use domain::{
    component::{Storefront, StorefrontOperation},
    game::{Game, GameId, GameVersion, GameVersionDetails, NewGame, SyncSummary},
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

        let games = match self.storefront.games().await {
            Ok(games) => games,
            Err(error) => {
                permit.finish(Err(error.clone()));
                return Err(error.into());
            }
        };

        let listed = games.len();
        let summary = self.store_games(games).await?;
        permit.finish(Ok(()));

        info!(
            component = self.component.id(),
            listed,
            inserted = summary.inserted,
            updated = summary.updated,
            restored = summary.restored,
            removed = summary.removed,
            "games synced"
        );

        if !summary.changed() {
            return Ok(());
        }

        event::emit(Topic::Games);

        if summary.inserted > 0 || summary.restored > 0 {
            self.component.context().artwork_service.trigger_backfill();
        }

        Ok(())
    }

    pub async fn list_game_versions(
        &self,
        game_id: GameId,
        path: Vec<String>,
    ) -> Result<Vec<GameVersion>> {
        let permit = Permit::begin(&self.component, StorefrontOperation::ListVersions)?;

        let game = self.load_game(game_id).await?;
        let versions = match self.storefront.game_versions(game, path).await {
            Ok(versions) => versions,
            Err(error) => {
                permit.finish(Err(error.clone()));
                return Err(error.into());
            }
        };
        permit.finish(Ok(()));

        Ok(versions)
    }

    pub async fn game_version_details(
        &self,
        game_id: GameId,
        path: Vec<String>,
    ) -> Result<GameVersionDetails> {
        let cache = self.component.version_details_cache();
        if let Some(details) = cache.get(game_id, &path) {
            return Ok(details);
        }

        let permit = Permit::begin(&self.component, StorefrontOperation::VersionDetails)?;

        let game = self.load_game(game_id).await?;
        let details = match self
            .storefront
            .game_version_details(game, path.clone())
            .await
        {
            Ok(details) => details,
            Err(error) => {
                permit.finish(Err(error.clone()));
                return Err(error.into());
            }
        };
        permit.finish(Ok(()));

        cache.insert(game_id, path, details.clone());
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

    async fn store_games(&self, games: Vec<NewGame>) -> Result<SyncSummary> {
        let repository = self.component.context().game_repository.clone();
        let source_id = self.component.id().to_owned();
        let summary = self
            .component
            .context()
            .tokio_handle
            .spawn_blocking(move || repository.sync(&source_id, &games))
            .await
            .map_err(anyhow::Error::from)?
            .inspect_err(
                |e| warn!(component = self.component.id(), error = %e, "storing games failed"),
            )?;

        Ok(summary)
    }
}

#[derive(Default)]
pub(crate) struct VersionDetailsCache(DashMap<(GameId, Vec<String>), (Moment, GameVersionDetails)>);

impl VersionDetailsCache {
    fn get(&self, game_id: GameId, path: &[String]) -> Option<GameVersionDetails> {
        let key = (game_id, path.to_vec());
        let now = Moment::now();
        self.0
            .remove_if(&key, |_, (expires_at, _)| expires_at.has_passed(now));
        self.0.get(&key).map(|entry| entry.1.clone())
    }

    fn insert(&self, game_id: GameId, path: Vec<String>, details: GameVersionDetails) {
        if let Some(expires_at) = Moment::now().checked_add(VERSION_DETAILS_TTL) {
            self.0.insert((game_id, path), (expires_at, details));
        }
    }

    pub(crate) fn clear(&self) {
        self.0.clear();
    }
}
