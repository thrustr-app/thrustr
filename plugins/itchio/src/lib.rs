use api::Classification;
use pdk::{
    Auth, ComboboxOption, Config, Error, Game, GameVersion, GameVersionDetails, LoginFlow,
    LoginRequest, Plugin, Storefront, config::Config as ConfigStore, kv_store::KvStore,
};
use std::collections::{BTreeMap, HashSet};

mod api;
mod oauth;

type Result<T> = std::result::Result<T, Error>;

const API_KEY: &str = "api_key";
const CLASSIFICATIONS: &str = "classifications";

pub struct Itchio;

#[pdk::export]
impl Plugin for Itchio {
    async fn init() -> Result<()> {
        api::profile(&api_key()?).await
    }
}

#[pdk::export]
impl Auth for Itchio {
    async fn login_flow() -> Result<Option<LoginFlow>> {
        oauth::authorization_flow().map(Some)
    }

    async fn login(request: LoginRequest) -> Result<()> {
        let LoginRequest::Flow(callback) = request else {
            return Err(Error::auth("expected an OAuth redirect"));
        };

        let api_key = oauth::complete(&callback.url).await?;
        api::profile(&api_key).await?;

        KvStore::set(API_KEY, &api_key)
    }

    async fn logout() -> Result<()> {
        KvStore::delete(API_KEY)
    }
}

#[pdk::export]
impl Config for Itchio {
    #[options("collections")]
    async fn collections() -> Result<Vec<ComboboxOption>> {
        let collections = api::collections(&api_key()?).await?;
        Ok(collections
            .into_iter()
            .map(|collection| ComboboxOption {
                value: collection.id.to_string(),
                label: collection.title,
            })
            .collect())
    }
}

#[pdk::export]
impl Storefront for Itchio {
    async fn list_games() -> Result<Vec<Game>> {
        let classifications = classifications()?;
        let keys = api::owned_keys(&api_key()?).await?;

        let mut seen = HashSet::new();

        Ok(keys
            .into_iter()
            .filter_map(|key| key.game)
            .filter(|game| classifications.contains(&game.classification))
            .filter(|game| seen.insert(game.id))
            .map(|game| Game {
                name: game.title,
                lookup_id: game.id.to_string(),
                external_ids: BTreeMap::new(),
                cover_url: non_empty(game.still_cover_url).or(non_empty(game.cover_url)),
                summary: non_empty(game.short_text),
                description: None,
            })
            .collect())
    }

    async fn list_game_versions(_game: Game) -> Result<Vec<GameVersion>> {
        Err(Error::other("not implemented"))
    }

    async fn game_version_details(_game: Game, _version_id: String) -> Result<GameVersionDetails> {
        Err(Error::other("not implemented"))
    }
}

fn api_key() -> Result<String> {
    KvStore::get(API_KEY)?.ok_or(Error::auth("not logged in"))
}

fn classifications() -> Result<HashSet<Classification>> {
    Ok(ConfigStore::get_list(CLASSIFICATIONS)?
        .iter()
        .map(|value| Classification::parse(value))
        .collect())
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.is_empty())
}
