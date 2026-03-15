use crate::api::error::Error;
use serde::Deserialize;
use std::fmt;

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Ok,
    Error,
}

#[derive(Debug, Deserialize)]
pub struct StructuredError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum ErrorData {
    Message(String),
    Structured(StructuredError),
}

impl fmt::Display for ErrorData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Message(m) => write!(f, "{m}"),
            Self::Structured(e) => write!(f, "[{}] {}", e.code, e.message),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum ApiResponse<T> {
    Ok { data: T },
    Error { data: ErrorData },
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct IsExistsByEmailSuccess {
    #[serde(rename = "giveawayUser")]
    pub giveaway_user: GiveawayUser,
    #[serde(rename = "wpUser")]
    pub wp_user: WpUser,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum GiveawayUser {
    None(bool),
    Response(GiveawayUserResponse),
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct GiveawayUserResponse {
    pub status: Status,
    pub data: Vec<GiveawayItem>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct GiveawayItem {
    pub product_id: String,
    pub game_id: String,
    pub installer_uuid: String,
    pub order_id: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum WpUser {
    None(bool),
    User(WpUserData),
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct WpUserData {
    pub id: u64,
    pub user_login: String,
    pub nickname: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginSuccess {
    #[serde(rename = "userId")]
    pub user_id: u64,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct Game {
    pub game_id: String,
    pub game_name: String,
    pub game_description: String,
    pub game_coverart: String,
    pub game_installed_size: String,
    pub installer_uuid: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct Product {
    pub id: u64,
    pub name: String,
    pub product_id: u64,
    pub games: Vec<Game>,
    pub purchasable: bool,
    pub catalog_visibility: String,
}

pub type IsExistsByEmailResponse = ApiResponse<IsExistsByEmailSuccess>;

impl IsExistsByEmailResponse {
    pub fn into_result(self) -> Result<(), Error> {
        match self {
            Self::Ok { data, .. } => match data.giveaway_user {
                GiveawayUser::Response(r) if r.status == Status::Ok => Ok(()),
                _ => Err(Error::UserNotFound),
            },
            Self::Error { data, .. } => Err(Error::Other(data.to_string())),
        }
    }
}

pub type LoginResponse = ApiResponse<LoginSuccess>;

impl LoginResponse {
    pub fn into_result(self) -> Result<LoginSuccess, Error> {
        match self {
            Self::Ok { data } => Ok(data),
            Self::Error { .. } => Err(Error::InvalidCredentials),
        }
    }
}

pub type ProductsResponse = ApiResponse<Vec<Product>>;

impl ProductsResponse {
    pub fn into_result(self) -> Result<Vec<Product>, Error> {
        match self {
            Self::Ok { data } => Ok(data),
            Self::Error { data, .. } => Err(Error::Other(data.to_string())),
        }
    }
}
