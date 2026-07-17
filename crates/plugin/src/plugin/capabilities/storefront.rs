use crate::{plugin::Plugin, wit::thrustr::plugin::types};
use async_trait::async_trait;
use domain::{
    component::{Error, capabilities::Storefront},
    game::{Game, GameSource, GameVersion, NewGame},
    platform::Platform,
};

#[async_trait]
impl Storefront for Plugin {
    async fn list_games(&self) -> Result<Vec<NewGame>, Error> {
        let games = self
            .call(|instance, mut store| async move {
                store
                    .run_concurrent(async |accessor| {
                        instance
                            .thrustr_plugin_storefront()
                            .call_get_games(accessor)
                            .await
                    })
                    .await
                    .and_then(|result| result)
            })
            .await?;

        Ok(games.into_iter().map(|g| self.to_new_game(g)).collect())
    }

    async fn list_game_versions(&self, game: Game) -> Result<Vec<GameVersion>, Error> {
        let versions = self
            .call(|instance, mut store| async move {
                store
                    .run_concurrent(async |accessor| {
                        instance
                            .thrustr_plugin_storefront()
                            .call_get_game_versions(accessor, game.into())
                            .await
                    })
                    .await
                    .and_then(|result| result)
            })
            .await?;

        Ok(versions.into_iter().map(Into::into).collect())
    }
}

impl Plugin {
    fn to_new_game(&self, game: types::Game) -> NewGame {
        NewGame {
            name: game.name,
            source: GameSource {
                id: self.manifest.plugin.id.clone(),
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
