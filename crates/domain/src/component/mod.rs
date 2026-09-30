use anyhow::Result;
use async_trait::async_trait;
use semver::Version;
use std::{collections::HashMap, sync::Arc};
use thiserror::Error;

mod capabilities;
mod form;
mod state;

pub use capabilities::*;
pub use form::*;
pub use state::*;

/// A component is a unit of functionality provided by the core application or by a plugin.
/// A component may expose one or more capabilities.
#[async_trait]
pub trait Component: Send + Sync {
    fn metadata(&self) -> Metadata<'_>;

    async fn init(&self) -> Result<(), Error>;

    fn auth(&self) -> Option<Arc<dyn Auth>> {
        None
    }

    fn config(&self) -> Option<Arc<dyn Config>> {
        None
    }

    fn storefront(&self) -> Option<Arc<dyn Storefront>> {
        None
    }

    fn scheduler(&self) -> Option<Arc<dyn Scheduler>> {
        None
    }
}

#[derive(Debug)]
pub struct Metadata<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub origin: Origin,
    pub description: Option<&'a str>,
    pub icon: Option<&'a Image>,
    pub version: &'a Version,
    pub authors: &'a [String],
}

#[derive(Debug, PartialEq, Eq)]
pub enum Origin {
    Core,
    Plugin,
}

impl Origin {
    pub fn is_plugin(&self) -> bool {
        matches!(self, Self::Plugin)
    }
}

#[derive(Debug, Clone)]
pub struct Image {
    pub bytes: Vec<u8>,
    pub format: ImageFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Webp,
    Gif,
    Svg,
    Bmp,
    Tiff,
    Ico,
    Pnm,
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
            "tiff" | "tif" => Some(Self::Tiff),
            "ico" => Some(Self::Ico),
            "pnm" | "pbm" | "ppm" | "pgm" => Some(Self::Pnm),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Error {
    #[error("authentication error: {0}")]
    Auth(String),

    #[error("configuration error: {0}")]
    Config(String),

    #[error("error: {0}")]
    Other(String),
}

impl Error {
    pub fn is_fatal(&self) -> bool {
        matches!(self, Error::Auth(_) | Error::Config(_))
    }
}

pub trait ComponentStorage: Send + Sync {
    fn get_data(&self, component_id: &str, key: &str) -> Result<Option<Vec<u8>>>;

    fn set_data(&self, component_id: &str, key: &str, value: &[u8]) -> Result<()>;

    fn delete_data(&self, component_id: &str, key: &str) -> Result<()>;

    fn list_data(&self, component_id: &str, prefix: Option<&str>) -> Result<Vec<String>>;

    fn get_config_value(&self, component_id: &str, field_id: &str) -> Result<Option<String>>;

    fn get_config_values(&self, component_id: &str) -> Result<HashMap<String, String>>;

    fn set_config_values(&self, component_id: &str, fields: &HashMap<String, String>)
    -> Result<()>;
}
