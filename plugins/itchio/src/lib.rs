use api::{Classification, Storage, UploadTrait};
use futures::future;
use pdk::{
    Auth, ComboboxOption, Config, Error, Game, GameVersion, GameVersionDetails, LoginFlow,
    LoginRequest, Platform, Plugin, Storefront, config::Config as ConfigStore, kv_store::KvStore,
};
use std::collections::HashSet;

mod api;
mod oauth;

type Result<T> = std::result::Result<T, Error>;

const API_KEY: &str = "api_key";
const CLASSIFICATIONS: &str = "classifications";
const COLLECTIONS: &str = "collections";
const DOWNLOAD_KEY_ID: &str = "download_key_id";

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
        let api_key = api_key()?;
        let classifications = classifications()?;
        let collection_ids = collection_ids()?;

        let (keys, collections) = futures::try_join!(
            api::owned_keys(&api_key),
            future::try_join_all(
                collection_ids
                    .into_iter()
                    .map(|id| api::collection_games(&api_key, id)),
            ),
        )?;

        let mut seen = HashSet::new();

        let owned = keys
            .into_iter()
            .filter_map(|key| Some((key.game?, Some(key.id))));
        let collected = collections.into_iter().flatten().map(|game| (game, None));

        Ok(owned
            .chain(collected)
            .filter(|(game, _)| classifications.contains(&game.classification))
            .filter(|(game, _)| seen.insert(game.id))
            .map(|(game, download_key_id)| Game {
                name: game.title,
                lookup_id: game.id.to_string(),
                external_ids: download_key_id
                    .map(|id| (DOWNLOAD_KEY_ID.to_string(), id.to_string()))
                    .into_iter()
                    .collect(),
                cover_url: non_empty(game.still_cover_url).or(non_empty(game.cover_url)),
                summary: non_empty(game.short_text),
                description: None,
            })
            .collect())
    }

    async fn list_game_versions(game: Game) -> Result<Vec<GameVersion>> {
        let download_key_id = game.external_ids.get(DOWNLOAD_KEY_ID).map(String::as_str);
        let uploads = api::uploads(&api_key()?, &game.lookup_id, download_key_id).await?;

        Ok(uploads
            .into_iter()
            .filter(|upload| {
                upload.storage != Storage::External
                    && !upload.traits.contains(&UploadTrait::Preorder)
            })
            .flat_map(|upload| {
                let id = upload.id.to_string();
                let name = non_empty(upload.display_name).unwrap_or(upload.filename);

                let mut platforms: Vec<_> = upload
                    .traits
                    .into_iter()
                    .filter_map(|upload_trait| match upload_trait {
                        UploadTrait::PWindows => Some(Platform::Windows),
                        UploadTrait::PLinux => Some(Platform::Linux),
                        UploadTrait::POsx => Some(Platform::Macos),
                        UploadTrait::Preorder | UploadTrait::Other => None,
                    })
                    .collect();
                if platforms.is_empty() {
                    platforms.push(Platform::Other);
                }

                platforms.into_iter().map(move |platform| GameVersion {
                    id: id.clone(),
                    pretty_name: Some(name.clone()),
                    platform,
                })
            })
            .collect())
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

fn collection_ids() -> Result<Vec<u64>> {
    ConfigStore::get_list(COLLECTIONS)?
        .iter()
        .map(|value| {
            value
                .parse()
                .map_err(|_| Error::other(format!("invalid collection id `{value}`")))
        })
        .collect()
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.is_empty())
}
