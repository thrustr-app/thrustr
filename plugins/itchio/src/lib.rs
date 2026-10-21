mod api;
mod oauth;
mod zip;

use api::{Build, BuildState, Classification, Storage, Upload, UploadTrait};
use futures::future;
use pdk::{
    Auth, ComboboxOption, Config, Error, Game, GameVersion, GameVersionDetails, LoginFlow,
    LoginRequest, Platform, Plugin, Storefront, config::Config as ConfigStore, kv_store::KvStore,
};
use std::collections::HashSet;

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
    async fn games() -> Result<Vec<Game>> {
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

        let owned = keys
            .into_iter()
            .filter_map(|key| Some((key.game?, Some(key.id))));
        let collected = collections.into_iter().flatten().map(|game| (game, None));

        let mut seen = HashSet::new();
        Ok(owned
            .chain(collected)
            .filter(|(game, _)| classifications.contains(&game.classification))
            .filter(|(game, _)| seen.insert(game.id))
            .map(|(game, download_key_id)| to_game(game, download_key_id))
            .collect())
    }

    async fn game_versions(game: Game, path: Vec<String>) -> Result<Vec<GameVersion>> {
        let api_key = api_key()?;
        let download_key_id = download_key_id(&game);

        match path.as_slice() {
            [] => {
                let uploads = api::uploads(&api_key, &game.lookup_id, download_key_id).await?;
                Ok(uploads
                    .into_iter()
                    .filter(is_installable)
                    .flat_map(upload_versions)
                    .collect())
            }
            [upload_id] => {
                let builds = api::builds(&api_key, upload_id, download_key_id).await?;
                Ok(builds
                    .into_iter()
                    .filter(|build| build.state == BuildState::Completed)
                    .map(build_version)
                    .collect())
            }
            _ => Err(invalid_path(&path)),
        }
    }

    async fn game_version_details(game: Game, path: Vec<String>) -> Result<GameVersionDetails> {
        let api_key = api_key()?;
        let download_key_id = download_key_id(&game);
        let (upload_id, build_id) = match path.as_slice() {
            [upload_id] => (upload_id.as_str(), None),
            [upload_id, build_id] => (upload_id.as_str(), Some(build_id.as_str())),
            _ => return Err(invalid_path(&path)),
        };

        let build = async {
            match build_id {
                Some(id) => api::build(&api_key, id, download_key_id).await.map(Some),
                None => Ok(None),
            }
        };
        let (upload, build) =
            futures::try_join!(api::upload(&api_key, upload_id, download_key_id), build)?;

        let latest = build
            .as_ref()
            .is_none_or(|build| upload.build_id == Some(build.id));

        let download_size = build
            .as_ref()
            .and_then(Build::archive)
            .map(|archive| archive.size)
            .or(upload.size.filter(|_| latest));

        let install_size = match upload
            .launch_targets_extracted_size
            .filter(|&size| size > 0 && latest)
        {
            Some(size) => Some(size),
            None if is_zip(&upload) => {
                zip_extracted_size(&api_key, upload_id, build_id, download_key_id)
                    .await
                    .ok()
                    .flatten()
            }
            None => None,
        };

        Ok(GameVersionDetails {
            install_size,
            download_size,
        })
    }
}

fn to_game(game: api::Game, download_key_id: Option<u64>) -> Game {
    Game {
        name: game.title,
        lookup_id: game.id.to_string(),
        external_ids: download_key_id
            .map(|id| (DOWNLOAD_KEY_ID.to_string(), id.to_string()))
            .into_iter()
            .collect(),
        cover_url: non_empty(game.still_cover_url).or_else(|| non_empty(game.cover_url)),
        summary: non_empty(game.short_text),
        description: None,
    }
}

fn is_installable(upload: &Upload) -> bool {
    upload.storage != Storage::External && !upload.traits.contains(&UploadTrait::Preorder)
}

fn upload_versions(upload: Upload) -> impl Iterator<Item = GameVersion> {
    let id = upload.id.to_string();
    let name = non_empty(upload.display_name).unwrap_or(upload.filename);
    let children_label = (upload.storage == Storage::Build).then(|| "Build".to_string());

    let mut platforms: Vec<_> = upload.traits.into_iter().filter_map(platform).collect();
    if platforms.is_empty() {
        platforms.push(Platform::Other);
    }

    platforms.into_iter().map(move |platform| GameVersion {
        id: id.clone(),
        pretty_name: Some(name.clone()),
        platform,
        children_label: children_label.clone(),
    })
}

fn platform(upload_trait: UploadTrait) -> Option<Platform> {
    match upload_trait {
        UploadTrait::PWindows => Some(Platform::Windows),
        UploadTrait::PLinux => Some(Platform::Linux),
        UploadTrait::POsx => Some(Platform::Macos),
        UploadTrait::PAndroid => Some(Platform::Android),
        UploadTrait::Preorder | UploadTrait::Other => None,
    }
}

fn build_version(build: Build) -> GameVersion {
    let version =
        non_empty(build.user_version).unwrap_or_else(|| format!("Build {}", build.version));
    let pretty_name = match build.created_at.as_deref().and_then(|date| date.get(..10)) {
        Some(date) => format!("{version} ({date})"),
        None => version,
    };

    GameVersion {
        id: build.id.to_string(),
        pretty_name: Some(pretty_name),
        platform: Platform::Other,
        children_label: None,
    }
}

fn is_zip(upload: &Upload) -> bool {
    upload.storage == Storage::Build || upload.filename.to_ascii_lowercase().ends_with(".zip")
}

async fn zip_extracted_size(
    api_key: &str,
    upload_id: &str,
    build_id: Option<&str>,
    download_key_id: Option<&str>,
) -> Result<Option<u64>> {
    let url = match build_id {
        Some(build_id) => api::build_archive_url(api_key, build_id, download_key_id).await?,
        None => api::upload_download_url(api_key, upload_id, download_key_id).await?,
    };
    zip::extracted_size(async |range: &str| api::fetch_range(&url, range).await).await
}

fn invalid_path(path: &[String]) -> Error {
    Error::other(format!("unexpected version path {path:?}"))
}

fn api_key() -> Result<String> {
    KvStore::get(API_KEY)?.ok_or_else(|| Error::auth("not logged in"))
}

fn download_key_id(game: &Game) -> Option<&str> {
    game.external_ids.get(DOWNLOAD_KEY_ID).map(String::as_str)
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
