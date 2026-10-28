use crate::{
    plugin::{PluginRuntime, guest_call},
    wit::{exports::thrustr::plugin::storefront, thrustr::plugin::types},
};
use async_trait::async_trait;
use domain::{
    component::{Error, Storefront},
    download::DownloadReporter,
    game::{Game, GameSource, GameVersion, GameVersionDetails, NewGame},
    platform::Platform,
};
use std::{path::PathBuf, sync::Arc};

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
        path: Vec<String>,
    ) -> Result<Vec<GameVersion>, Error> {
        let versions = guest_call!(self.runtime, self.indices, |storefront, accessor| {
            storefront.call_get_game_versions(accessor, game.into(), path)
        })?;

        Ok(versions.into_iter().map(Into::into).collect())
    }

    async fn game_version_details(
        &self,
        game: Game,
        path: Vec<String>,
    ) -> Result<GameVersionDetails, Error> {
        let details = guest_call!(self.runtime, self.indices, |storefront, accessor| {
            storefront.call_get_game_version_details(accessor, game.into(), path)
        })?;

        Ok(details.into())
    }

    async fn download(
        &self,
        _game: Game,
        _path: Vec<String>,
        _target_dir: PathBuf,
        _reporter: Arc<dyn DownloadReporter>,
    ) -> Result<(), Error> {
        Err(Error::Other("downloads are not supported yet".into()))
    }
}

impl PluginStorefront {
    fn to_new_game(&self, game: types::Game) -> NewGame {
        NewGame {
            name: game.name,
            source: GameSource {
                id: self.runtime.id.clone(),
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
