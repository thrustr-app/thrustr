use crate::game::GameId;
use anyhow::Result;
use strum::{AsRefStr, Display, EnumString};

#[derive(AsRefStr, Display, EnumString, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[strum(serialize_all = "lowercase")]
pub enum ArtworkKind {
    Cover,
    Banner,
    Screenshot,
}

#[derive(Debug)]
pub struct Artwork {
    pub hash: String,
    pub kind: ArtworkKind,
    pub position: u32,
    pub accent_color: Option<Color>,
}

#[derive(Debug, Clone, Copy)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl From<u32> for Color {
    fn from(value: u32) -> Self {
        Self {
            r: ((value >> 16) & 0xFF) as u8,
            g: ((value >> 8) & 0xFF) as u8,
            b: (value & 0xFF) as u8,
        }
    }
}

impl From<Color> for u32 {
    fn from(color: Color) -> Self {
        ((color.r as u32) << 16) | ((color.g as u32) << 8) | color.b as u32
    }
}

pub trait ArtworkRepository: Send + Sync {
    fn insert(&self, game_id: GameId, artwork: &Artwork) -> Result<()>;
}
