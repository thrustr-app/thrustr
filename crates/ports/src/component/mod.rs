use crate::capabilities::Storefront;
use async_trait::async_trait;
use semver::Version;
use std::{fmt, sync::Arc};

mod config;

pub use config::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Webp,
    Gif,
    Svg,
    Bmp,
    Tiff,
}

impl ImageFormat {
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "png" => Some(Self::Png),
            "jpg" | "jpeg" => Some(Self::Jpeg),
            "webp" => Some(Self::Webp),
            "gif" => Some(Self::Gif),
            "svg" => Some(Self::Svg),
            "bmp" => Some(Self::Bmp),
            "tiff" => Some(Self::Tiff),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Image {
    pub bytes: Vec<u8>,
    pub format: ImageFormat,
}

#[derive(Debug)]
pub enum Origin {
    Core,
    Plugin(String),
}

impl Origin {
    pub fn is_core(&self) -> bool {
        matches!(self, Self::Core)
    }

    pub fn is_plugin(&self) -> bool {
        matches!(self, Self::Plugin(_))
    }

    pub fn plugin_id(&self) -> Option<&str> {
        match self {
            Self::Plugin(id) => Some(id),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Error {
    Configuration(String),
    Authentication(String),
    Other(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Configuration(msg) => write!(f, "Configuration error: {msg}"),
            Error::Authentication(msg) => write!(f, "Authentication error: {msg}"),
            Error::Other(msg) => write!(f, "Error: {msg}"),
        }
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Clone)]
pub enum Status {
    Inactive,
    Initializing,
    Active,
    InitError(Error),
    Unauthenticated,
    Error(Error),
}

impl Status {
    pub fn is_inactive(&self) -> bool {
        matches!(self, Self::Inactive)
    }

    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }

    pub fn is_initializing(&self) -> bool {
        matches!(self, Self::Initializing)
    }

    pub fn is_init_error(&self) -> bool {
        matches!(self, Self::InitError(_))
    }

    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error(_))
    }

    pub fn is_any_error(&self) -> bool {
        matches!(self, Self::InitError(_) | Self::Error(_))
    }

    pub fn can_init(&self) -> bool {
        matches!(self, Self::Inactive | Self::InitError(_))
    }

    pub fn needs_login(&self) -> bool {
        matches!(self, Self::Unauthenticated)
    }
}

pub struct Metadata {
    pub id: String,
    pub name: String,
    pub origin: Origin,
    pub description: Option<String>,
    pub icon: Option<Image>,
    pub version: Version,
    pub authors: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct AuthFlow {
    pub url: String,
    pub target: String,
}

/// A component is a unit of functionality provided by the core application or by a plugin.
/// A component may expose one or more capabilities.
#[async_trait]
pub trait Component: Send + Sync {
    fn metadata(&self) -> &Metadata;
    fn status(&self) -> Status;
    fn set_status(&self, status: Status);
    fn config(&self) -> Option<&Config> {
        None
    }

    /// Returns a storefront capability instance if this component exposes one.
    fn storefront(self: Arc<Self>) -> Option<Arc<dyn Storefront>> {
        None
    }

    async fn init(&self) -> Result<(), Error>;
    async fn get_login_flow(&self) -> Result<Option<AuthFlow>, Error>;
    async fn get_logout_flow(&self) -> Result<Option<AuthFlow>, Error>;
    async fn login(&self, url: String, body: String) -> Result<(), Error>;
    async fn logout(&self, url: String, body: String) -> Result<(), Error>;
    async fn validate_config(&self, fields: &[(String, String)]) -> Result<(), Error>;
}
