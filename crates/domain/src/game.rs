use crate::{
    artwork::{Artwork, ArtworkKind},
    id::Id,
    platform::Platform,
};
use anyhow::Result;
use std::collections::HashMap;

pub type GameId = Id<Game>;

#[derive(Debug)]
pub struct Game {
    pub id: GameId,
    pub name: String,
    pub source: GameSource,
    /// The URL of the original cover art for the game, as provided by the storefront.
    pub cover_url: Option<String>,
    pub cover: Option<Artwork>,
    pub summary: Option<String>,
    pub description: Option<String>,
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
    pub summary: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug)]
pub struct GameListItem {
    pub id: GameId,
    pub name: String,
    pub source_id: String,
    pub cover_url: Option<String>,
    pub cover: Option<Artwork>,
}

pub trait GameRepository: Send + Sync {
    fn insert(&self, game: &NewGame) -> Result<Option<Game>>;

    fn insert_many(&self, games: &[NewGame]) -> Result<usize>;

    fn count(&self) -> Result<usize>;

    fn get(&self, id: GameId) -> Result<Option<Game>>;

    fn list(&self, offset: usize, limit: usize) -> Result<Vec<GameListItem>>;

    fn games_missing_artwork(
        &self,
        kind: ArtworkKind,
        after: GameId,
        limit: usize,
    ) -> Result<Vec<(GameId, String)>>;
}
