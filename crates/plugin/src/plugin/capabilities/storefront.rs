use crate::{
    plugin::{DownloadCall, PluginRuntime, guest_call, guest_fn},
    wit::{exports::thrustr::plugin::storefront, thrustr::plugin::types},
};
use async_trait::async_trait;
use domain::{
    component::{Error, Storefront},
    download::{DownloadOutcome, DownloadReporter},
    game::{Game, GameSource, GameVersion, GameVersionDetails, NewGame},
    platform::Platform,
};
use std::{path::PathBuf, sync::Arc};
use tokio_util::sync::CancellationToken;

pub struct PluginStorefront {
    runtime: Arc<PluginRuntime>,
    indices: storefront::GuestIndices,
}

impl PluginStorefront {
    pub fn resolve(runtime: &Arc<PluginRuntime>) -> anyhow::Result<Option<Self>> {
        let indices = runtime.export("storefront", storefront::GuestIndices::new)?;
        Ok(indices.map(|indices| Self {
            runtime: runtime.clone(),
            indices,
        }))
    }
}

#[async_trait]
impl Storefront for PluginStorefront {
    async fn games(&self) -> Result<Vec<NewGame>, Error> {
        let games = guest_call!(self.runtime, self.indices, |storefront, accessor| {
            storefront.call_get_games(accessor)
        })?;

        Ok(games.into_iter().map(|g| self.to_new_game(g)).collect())
    }

    async fn game_versions(
        &self,
        game: Game,
        version_path: Vec<String>,
    ) -> Result<Vec<GameVersion>, Error> {
        let versions = guest_call!(self.runtime, self.indices, |storefront, accessor| {
            storefront.call_get_game_versions(accessor, game.into(), version_path)
        })?;

        Ok(versions.into_iter().map(Into::into).collect())
    }

    async fn game_version_details(
        &self,
        game: Game,
        version_path: Vec<String>,
    ) -> Result<GameVersionDetails, Error> {
        let details = guest_call!(self.runtime, self.indices, |storefront, accessor| {
            storefront.call_get_game_version_details(accessor, game.into(), version_path)
        })?;

        Ok(details.into())
    }

    async fn download(
        &self,
        game: Game,
        version_path: Vec<String>,
        target_dir: PathBuf,
        reporter: Arc<dyn DownloadReporter>,
        cancel: CancellationToken,
    ) -> Result<DownloadOutcome, Error> {
        let call = guest_fn!(self.indices, |storefront, accessor| async {
            let reporter = accessor.with(|mut access| access.get().reporter())?;
            storefront
                .call_download(accessor, game.into(), version_path, reporter)
                .await
        });

        let runtime = self.runtime.clone();
        self.runtime
            .spawn(async move {
                tokio::fs::create_dir_all(&target_dir).await.map_err(|e| {
                    Error::Other(format!("creating {} failed: {e}", target_dir.display()))
                })?;
                let download = DownloadCall {
                    dir: target_dir,
                    reporter,
                };
                Ok(match runtime.run(Some(download), &cancel, call).await? {
                    Some(()) => DownloadOutcome::Completed,
                    None => DownloadOutcome::Cancelled,
                })
            })
            .await
    }
}

impl PluginStorefront {
    fn to_new_game(&self, game: types::Game) -> NewGame {
        NewGame {
            name: game.name,
            source: GameSource {
                id: self.runtime.id.to_string(),
                lookup_id: game.lookup_id,
                external_ids: game.external_ids,
            },
            cover_url: game.cover_url,
            summary: game.summary,
            description: game.description,
        }
    }
}

impl From<types::GameVersion> for GameVersion {
    fn from(value: types::GameVersion) -> Self {
        GameVersion {
            id: value.id,
            pretty_name: value.pretty_name,
            platform: value.platform.into(),
            children_label: value.children_label,
        }
    }
}

impl From<types::GameVersionDetails> for GameVersionDetails {
    fn from(value: types::GameVersionDetails) -> Self {
        GameVersionDetails {
            install_size: value.install_size,
            download_size: value.download_size,
        }
    }
}

impl From<types::Platform> for Platform {
    fn from(value: types::Platform) -> Self {
        match value {
            types::Platform::Windows => Platform::Windows,
            types::Platform::Linux => Platform::Linux,
            types::Platform::Macos => Platform::Macos,
            types::Platform::Android => Platform::Android,
            types::Platform::Other => Platform::Other,
        }
    }
}

impl From<Game> for types::Game {
    fn from(value: Game) -> Self {
        types::Game {
            name: value.name,
            lookup_id: value.source.lookup_id,
            external_ids: value.source.external_ids.into_iter().collect(),
            cover_url: value.cover_url,
            summary: value.summary,
            description: value.description,
        }
    }
}
