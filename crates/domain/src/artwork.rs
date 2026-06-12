use crate::game::GameId;
use anyhow::Result;
use strum::{AsRefStr, Display, EnumString};

#[derive(AsRefStr, Display, EnumString, Debug, Clone, Copy)]
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

impl Color {
    pub fn to_hex(self) -> u32 {
        ((self.r as u32) << 16) | ((self.g as u32) << 8) | self.b as u32
    }

    pub fn from_hex(value: u32) -> Self {
        Self {
            r: ((value >> 16) & 0xFF) as u8,
            g: ((value >> 8) & 0xFF) as u8,
            b: (value & 0xFF) as u8,
        }
    }
}

pub trait ArtworkRepository: Send + Sync {
    fn insert(&self, game_id: GameId, artwork: &Artwork) -> Result<()>;
}
