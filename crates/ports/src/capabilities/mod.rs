use async_trait::async_trait;
use semver::Version;

mod storefront;

pub use storefront::*;

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

pub struct Image {
    pub bytes: Vec<u8>,
    pub format: ImageFormat,
}

#[derive(Debug)]
pub enum ComponentOrigin {
    Core,
    Plugin(String),
}

impl ComponentOrigin {
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
pub enum ComponentError {
    Initialization(String),
    Runtime(String),
}

#[derive(Debug, Clone)]
pub enum ComponentStatus {
    Inactive,
    Active,
    Initializing,
    Error(ComponentError),
}

impl ComponentStatus {
    pub fn is_inactive(&self) -> bool {
        matches!(self, Self::Inactive)
    }

    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }

    pub fn is_initializing(&self) -> bool {
        matches!(self, Self::Initializing)
    }

    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error(_))
    }
}

pub struct ComponentMetadata {
    pub id: String,
    pub name: String,
    pub origin: ComponentOrigin,
    pub description: Option<String>,
    pub icon: Option<Image>,
    pub version: Version,
    pub authors: Vec<String>,
}

#[async_trait]
pub trait Component: Send + Sync {
    fn metadata(&self) -> &ComponentMetadata;

    fn status(&self) -> ComponentStatus;
    async fn init(&self) -> Result<(), ComponentError>;
}
