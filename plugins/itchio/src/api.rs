use pdk::Error;
use serde::{
    Deserialize, Deserializer,
    de::{
        self, DeserializeOwned, IgnoredAny, MapAccess, SeqAccess, Visitor,
        value::SeqAccessDeserializer,
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
pub struct OwnedKey {
    pub game: Option<OwnedGame>,
}

#[derive(Deserialize)]
pub struct OwnedGame {
    pub id: u64,
    pub title: String,
    #[serde(default)]
    pub classification: Classification,
    pub short_text: Option<String>,
    pub cover_url: Option<String>,
    pub still_cover_url: Option<String>,
}

#[derive(Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Classification {
    #[default]
    Game,
    Tool,
    #[serde(other)]
    Other,
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
