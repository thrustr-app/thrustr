use semver::Version;
use serde::Deserialize;
use crate::component::{Error, Field};

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

    pub fn can_login(&self) -> bool {
        matches!(
            self,
            Self::Unauthenticated | Self::Error(Error::Auth(_)) | Self::InitError(Error::Auth(_))
        )
    }

    pub fn can_logout(&self) -> bool {
        matches!(self, Self::Active)
            || matches!(
                self,
                Self::Error(e) | Self::InitError(e)
                if !matches!(e, Error::Auth(_))
            )
    }

    pub fn can_configure(&self) -> bool {
        matches!(
            self,
            Self::Active
                | Self::Unauthenticated
                | Self::Error(Error::Config(_))
                | Self::InitError(Error::Config(_))
        )
    }

    pub fn error_message(&self) -> Option<String> {
        match self {
            Self::InitError(e) | Self::Error(e) => Some(e.to_string()),
            _ => None,
        }
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

#[derive(Deserialize, Debug, Clone)]
pub struct LoginForm {
    #[serde(rename = "field")]
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone)]
pub enum LoginMethod {
    Flow(AuthFlow),
    Form(LoginForm),
}

#[derive(Debug, Clone)]
pub struct AuthFlow {
    pub url: String,
    pub target: String,
}
