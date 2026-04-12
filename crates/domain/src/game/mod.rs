use crate::id::Id;
use std::collections::HashMap;

mod commands;
mod projections;
mod repository;

pub use commands::*;
pub use projections::*;
pub use repository::*;

#[derive(Debug)]
pub struct Game {
    pub id: Id<Self>,
    pub name: String,
    pub source: GameSource,
}

#[derive(Debug)]
pub struct GameSource {
    /// The identifier for the game source (e.g. "steam", "gog").
    pub source_id: String,
    /// The unique identifier for the game in the source. This usually is a specific
    /// identifier (e.g. Steam App ID) or a combination of multiple identifiers.
    pub lookup_id: String,
    /// Arbitrary external identifiers to be consumed by components.
    pub external_ids: HashMap<String, String>,
}
