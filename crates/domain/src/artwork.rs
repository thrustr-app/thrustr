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
    pub a: u8,
}

impl Color {
    pub const BLACK: Color = Color::rgb(0, 0, 0);
    pub const WHITE: Color = Color::rgb(255, 255, 255);

    pub const fn rgb(r: u8, g: u8, b: u8) -> Color {
        Color { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
        Color { r, g, b, a }
    }

    pub fn from_argb_hex(argb: u32) -> Self {
        Self {
            a: (argb >> 24) as u8,
            r: ((argb >> 16) & 0xFF) as u8,
            g: ((argb >> 8) & 0xFF) as u8,
            b: (argb & 0xFF) as u8,
        }
    }

    pub fn to_argb_hex(self) -> u32 {
        ((self.a as u32) << 24) | ((self.r as u32) << 16) | ((self.g as u32) << 8) | self.b as u32
    }

    pub fn to_rgba_hex(self) -> u32 {
        ((self.r as u32) << 24) | ((self.g as u32) << 16) | ((self.b as u32) << 8) | self.a as u32
    }

    pub fn to_rgba(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }

    pub fn to_argb(self) -> [u8; 4] {
        [self.a, self.r, self.g, self.b]
    }

    /// Normalizes the color channels to the `[0.0, 1.0]` range.
    pub fn normalized(self) -> (f32, f32, f32, f32) {
        (
            self.r as f32 / 255.0,
            self.g as f32 / 255.0,
            self.b as f32 / 255.0,
            self.a as f32 / 255.0,
        )
    }
}

pub trait ArtworkRepository: Send + Sync {
    fn insert(&self, game_id: GameId, artwork: &Artwork) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEAL: Color = Color::rgb(0x12, 0x34, 0x56);

    #[test]
    fn colors_pack_into_rgba_order() {
        assert_eq!(TEAL.to_argb_hex(), 0xFF123456);
        assert_eq!(Color::rgba(0, 0, 0xFF, 0x80).to_argb_hex(), 0x800000FF);
    }

    #[test]
    fn colors_unpack_into_rgba_components() {
        assert_eq!(Color::from_argb_hex(0xFF123456), TEAL);
        assert_eq!(
            Color::from_argb_hex(0x80123456),
            Color::rgba(0x12, 0x34, 0x56, 0x80)
        );
    }

    #[test]
    fn colors_survive_a_roundtrip_through_u32() {
        assert_eq!(Color::from_argb_hex(TEAL.to_argb_hex()), TEAL);
    }
}
