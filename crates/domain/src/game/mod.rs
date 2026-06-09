use crate::{artwork::Artwork, id::Id, platform::Platform};
use std::collections::HashMap;

mod repository;

pub use repository::*;

#[derive(Debug)]
pub struct Game {
    pub id: Id<Self>,
    pub name: String,
    pub source: GameSource,
    /// The URL of the original cover art for the game, as provided by the storefront.
    pub cover_url: Option<String>,
    pub cover: Option<Artwork>,
}

#[derive(Debug)]
pub struct GameSource {
    /// The identifier for the game source (e.g. "steam", "gog").
    pub id: String,
    /// The unique identifier for the game in the source. This usually is a specific
    /// identifier (e.g. Steam App ID) or a combination of multiple identifiers.
    pub lookup_id: String,
    /// Arbitrary external identifiers to be consumed by components.
    pub external_ids: HashMap<String, String>,
}

#[derive(Debug)]
pub struct GameVersion {
    pub id: String,
    pub pretty_name: Option<String>,
    pub platform: Platform,
}

#[derive(Debug)]
pub struct NewGame {
    pub name: String,
    pub source: GameSource,
    pub cover_url: Option<String>,
}
