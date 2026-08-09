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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[cfg(test)]
mod tests {
    use super::*;

    const TEAL: Color = Color {
        r: 0x12,
        g: 0x34,
        b: 0x56,
    };

    #[test]
    fn colors_pack_into_rgb_order() {
        assert_eq!(u32::from(TEAL), 0x123456);
        assert_eq!(
            u32::from(Color {
                r: 0,
                g: 0,
                b: 0xFF
            }),
            0xFF
        );
    }

    #[test]
    fn colors_unpack_ignoring_the_high_byte() {
        assert_eq!(Color::from(0x123456), TEAL);
        assert_eq!(Color::from(0xFF123456), TEAL);
    }

    #[test]
    fn colors_survive_a_roundtrip_through_u32() {
        assert_eq!(Color::from(u32::from(TEAL)), TEAL);
    }
}
