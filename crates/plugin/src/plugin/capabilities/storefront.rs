use crate::{
    plugin::{PluginRuntime, guest_call},
    wit::{exports::thrustr::plugin::storefront, thrustr::plugin::types},
};
use async_trait::async_trait;
use domain::{
    component::{Error, Storefront},
    game::{Game, GameSource, GameVersion, NewGame},
    platform::Platform,
};
use std::sync::Arc;

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
    async fn list_games(&self) -> Result<Vec<NewGame>, Error> {
        let games = guest_call!(self.runtime, self.indices, |storefront, accessor| {
            storefront.call_get_games(accessor)
        })?;

        Ok(games.into_iter().map(|g| self.to_new_game(g)).collect())
    }

    async fn list_game_versions(&self, game: Game) -> Result<Vec<GameVersion>, Error> {
        let versions = guest_call!(self.runtime, self.indices, |storefront, accessor| {
            storefront.call_get_game_versions(accessor, game.into())
        })?;

        Ok(versions.into_iter().map(Into::into).collect())
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
        }
    }
}

impl From<types::Platform> for Platform {
    fn from(value: types::Platform) -> Self {
        match value {
            types::Platform::Windows => Platform::Windows,
            types::Platform::Linux => Platform::Linux,
            types::Platform::Macos => Platform::Macos,
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
