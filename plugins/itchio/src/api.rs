use pdk::Error;
use serde::{
    Deserialize, Deserializer,
    de::{
        self, DeserializeOwned, IgnoredAny, IntoDeserializer, MapAccess, SeqAccess, Visitor,
        value::{SeqAccessDeserializer, StrDeserializer},
    },
};
use std::{fmt, marker::PhantomData};
use wasi_fetch::{Client, RequestBuilder};

const BASE_URL: &str = "https://api.itch.io";
const MAX_PER_PAGE: u32 = 500;

pub async fn profile(api_key: &str) -> Result<(), Error> {
    let request = get("/profile").header("Authorization", api_key);
    send::<IgnoredAny>(request).await.map(drop)
}

pub async fn owned_keys(api_key: &str) -> Result<Vec<OwnedKey>, Error> {
    let mut keys = Vec::new();

    for page in 1.. {
        let request = get(&format!(
            "/profile/owned-keys?page={page}&per_page={MAX_PER_PAGE}"
        ))
        .header("Authorization", api_key);
        let response: OwnedKeysPage = send(request).await?;

        let count = response.owned_keys.len();
        keys.extend(response.owned_keys);

        if count == 0 || count < response.per_page {
            break;
        }
    }

    Ok(keys)
}

pub async fn collections(api_key: &str) -> Result<Vec<Collection>, Error> {
    let request = get("/profile/collections").header("Authorization", api_key);
    send::<CollectionsResponse>(request)
        .await
        .map(|response| response.collections)
}

pub async fn collection_games(api_key: &str, collection_id: u64) -> Result<Vec<Game>, Error> {
    let mut games = Vec::new();

    // Less than the requested `per_page` can be returned if games are hidden, only an empty page marks the end.
    for page in 1.. {
        let request = get(&format!(
            "/collections/{collection_id}/collection-games?page={page}&per_page={MAX_PER_PAGE}"
        ))
        .header("Authorization", api_key);
        let response: CollectionGamesPage = send(request).await?;

        if response.collection_games.is_empty() {
            break;
        }

        games.extend(
            response
                .collection_games
                .into_iter()
                .filter_map(|entry| entry.game),
        );
    }

    Ok(games)
}

pub async fn uploads(
    api_key: &str,
    game_id: &str,
    download_key_id: Option<&str>,
) -> Result<Vec<Upload>, Error> {
    let mut path = format!("/games/{game_id}/uploads");
    if let Some(download_key_id) = download_key_id {
        path.push_str(&format!("?download_key_id={download_key_id}"));
    }

    let request = get(&path).header("Authorization", api_key);
    send::<UploadsResponse>(request)
        .await
        .map(|response| response.uploads)
}

pub fn get(path: &str) -> RequestBuilder {
    request(Client::new().get(&format!("{BASE_URL}{path}")))
}

pub fn post(path: &str) -> RequestBuilder {
    request(Client::new().post(&format!("{BASE_URL}{path}")))
}

fn request(builder: RequestBuilder) -> RequestBuilder {
    builder.header("Accept", "application/vnd.itch.v2")
}

pub async fn send<T: DeserializeOwned>(request: RequestBuilder) -> Result<T, Error> {
    let response = request.send().await.map_err(request_error)?;
    let status = response.status();
    let body = response.into_body().bytes().await;

    if let Ok(ApiErrors { errors }) = serde_json::from_slice(&body) {
        let message = errors.join(", ");
        return Err(match status.as_u16() {
            401 | 403 => Error::auth(message),
            _ => Error::other(message),
        });
    }

    serde_json::from_slice(&body)
        .map_err(|e| Error::other(format!("unexpected response ({status}): {e}")))
}

fn request_error(error: wasi_fetch::Error) -> Error {
    Error::other(format!("request failed: {error}"))
}

#[derive(Deserialize)]
struct ApiErrors {
    errors: Vec<String>,
}

#[derive(Deserialize)]
struct OwnedKeysPage {
    per_page: usize,
    #[serde(deserialize_with = "list_or_empty_object")]
    owned_keys: Vec<OwnedKey>,
}

#[derive(Deserialize)]
struct CollectionsResponse {
    #[serde(deserialize_with = "list_or_empty_object")]
    collections: Vec<Collection>,
}

#[derive(Deserialize)]
pub struct Collection {
    pub id: u64,
    pub title: String,
}

#[derive(Deserialize)]
struct CollectionGamesPage {
    #[serde(deserialize_with = "list_or_empty_object")]
    collection_games: Vec<CollectionGame>,
}

#[derive(Deserialize)]
struct CollectionGame {
    game: Option<Game>,
}

#[derive(Deserialize)]
pub struct OwnedKey {
    pub id: u64,
    pub game: Option<Game>,
}

#[derive(Deserialize)]
pub struct Game {
    pub id: u64,
    pub title: String,
    #[serde(default)]
    pub classification: Classification,
    pub short_text: Option<String>,
    pub cover_url: Option<String>,
    pub still_cover_url: Option<String>,
}

#[derive(Deserialize)]
struct UploadsResponse {
    #[serde(deserialize_with = "list_or_empty_object")]
    uploads: Vec<Upload>,
}

#[derive(Deserialize)]
pub struct Upload {
    pub id: u64,
    #[serde(default)]
    pub filename: String,
    pub display_name: Option<String>,
    pub storage: Storage,
    #[serde(default, deserialize_with = "list_or_empty_object")]
    pub traits: Vec<UploadTrait>,
}

#[derive(Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Storage {
    Hosted,
    Build,
    External,
    #[serde(other)]
    Other,
}

#[derive(Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UploadTrait {
    PWindows,
    PLinux,
    POsx,
    Preorder,
    #[serde(other)]
    Other,
}

#[derive(Deserialize, Default, PartialEq, Eq, Hash, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum Classification {
    #[default]
    Game,
    Tool,
    Assets,
    GameMod,
    PhysicalGame,
    Soundtrack,
    Comic,
    Book,
    #[serde(other)]
    Other,
}

impl Classification {
    pub fn parse(value: &str) -> Self {
        let deserializer: StrDeserializer<'_, de::value::Error> = value.into_deserializer();
        Self::deserialize(deserializer).unwrap_or(Self::Other)
    }
}

fn list_or_empty_object<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct ListVisitor<T>(PhantomData<T>);

    impl<'de, T: Deserialize<'de>> Visitor<'de> for ListVisitor<T> {
        type Value = Vec<T>;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a list or an empty object")
        }

        fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<Vec<T>, A::Error> {
            Vec::deserialize(SeqAccessDeserializer::new(seq))
        }

        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Vec<T>, A::Error> {
            match map.next_key::<IgnoredAny>()? {
                None => Ok(Vec::new()),
                Some(_) => Err(de::Error::invalid_value(de::Unexpected::Map, &self)),
            }
        }
    }

    deserializer.deserialize_any(ListVisitor(PhantomData))
}
