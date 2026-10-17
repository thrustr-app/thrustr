use pdk::{Error, kv_store::KvStore};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{fmt, time::Duration};
use url::form_urlencoded;
use wasi_fetch::{Client, RequestBuilder};

const BASE_URL: &str = "https://api.legacygames.com";

const CATALOG_KEY: &str = "catalog";
const CATALOG_TTL: Duration = Duration::from_hours(1);

pub async fn giveaway_login(email: &str) -> Result<(), Error> {
    let url = endpoint("/users/isexistsbyemail", &[("email", email)]);
    let user: UserLookup = fetch_data(request(&url)).await?;

    match user.giveaway_user {
        GiveawayUser::Found { status: Status::Ok } => Ok(()),
        _ => Err(Error::auth("user not found")),
    }
}

pub async fn login(token: &str) -> Result<u64, Error> {
    let url = endpoint("/users/login", &[]);
    let response: ApiResponse<LoginSuccess> = send(authenticated_request(&url, token)).await?;

    match response {
        ApiResponse::Ok { data } => Ok(data.user_id),
        ApiResponse::Error { .. } => Err(Error::auth("invalid credentials")),
    }
}

pub async fn giveaway_products(email: &str) -> Result<Vec<Product>, Error> {
    let url = endpoint("/users/getgiveawaycatalogbyemail", &[("email", email)]);
    fetch_data(request(&url)).await
}

pub async fn entitlements(token: &str, user_id: u64) -> Result<Vec<Entitlement>, Error> {
    let url = endpoint("/users/downloads", &[("userId", &user_id.to_string())]);
    fetch_data(authenticated_request(&url, token)).await
}

pub async fn catalog() -> Result<Vec<Product>, Error> {
    if let Ok(Some(catalog)) = KvStore::get(CATALOG_KEY) {
        return Ok(catalog);
    }

    let url = endpoint("/products/catalog", &[]);
    let catalog = send(request(&url)).await?;
    let _ = KvStore::set_with_ttl(CATALOG_KEY, &catalog, CATALOG_TTL);
    Ok(catalog)
}

pub async fn download_size(product_id: &str, game_id: &str) -> Result<Option<u64>, Error> {
    let url = endpoint(
        "/products/download",
        &[("productId", product_id), ("gameId", game_id)],
    );
    let download: Download = fetch_data(request(&url)).await?;

    let response = Client::new()
        .head(&download.file)
        .send()
        .await
        .map_err(request_error)?;

    if !response.status().is_success() {
        return Err(Error::other(format!(
            "installer request failed with status {}",
            response.status()
        )));
    }

    Ok(response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok()))
}

fn endpoint(path: &str, query: &[(&str, &str)]) -> String {
    let mut url = format!("{BASE_URL}{path}");

    if !query.is_empty() {
        url.push('?');
        url.push_str(
            &form_urlencoded::Serializer::new(String::new())
                .extend_pairs(query)
                .finish(),
        );
    }

    url
}

fn request(url: &str) -> RequestBuilder {
    Client::new()
        .get(url)
        .header("Authorization", "?token?")
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .header("Cache-Control", "no-cache")
}

fn authenticated_request(url: &str, token: &str) -> RequestBuilder {
    request(url).header("UserToken", format!("Basic {token}"))
}

async fn fetch_data<T: DeserializeOwned>(request: RequestBuilder) -> Result<T, Error> {
    match send(request).await? {
        ApiResponse::Ok { data } => Ok(data),
        ApiResponse::Error { data } => Err(Error::other(data.to_string())),
    }
}

async fn send<T: DeserializeOwned>(request: RequestBuilder) -> Result<T, Error> {
    let response = request.send().await.map_err(request_error)?;
    response.into_body().json().await.map_err(request_error)
}

fn request_error(error: wasi_fetch::Error) -> Error {
    Error::other(format!("request failed: {error}"))
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ApiResponse<T> {
    Ok { data: T },
    Error { data: ErrorData },
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ErrorData {
    Message(String),
    Structured { code: String, message: String },
}

impl fmt::Display for ErrorData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Message(message) => write!(f, "{message}"),
            Self::Structured { code, message } => write!(f, "[{code}] {message}"),
        }
    }
}

#[derive(Deserialize)]
struct UserLookup {
    #[serde(rename = "giveawayUser")]
    giveaway_user: GiveawayUser,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum GiveawayUser {
    Found {
        status: Status,
    },
    #[expect(dead_code, reason = "deserialized to tell it apart from `Found`")]
    NotFound(bool),
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum Status {
    Ok,
    Error,
}

#[derive(Deserialize)]
struct LoginSuccess {
    #[serde(rename = "userId")]
    user_id: u64,
}

#[derive(Deserialize)]
struct Download {
    file: String,
}

#[derive(Serialize, Deserialize)]
pub struct Product {
    pub id: u64,
    #[serde(default)]
    pub games: Vec<ProductGame>,
}

#[derive(Serialize, Deserialize)]
pub struct ProductGame {
    pub game_id: String,
    pub game_name: String,
    pub game_description: String,
    pub game_coverart: String,
    pub installer_uuid: String,
}

#[derive(Deserialize)]
pub struct Entitlement {
    pub product_id: u64,
}
