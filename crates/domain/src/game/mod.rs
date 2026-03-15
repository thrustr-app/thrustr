use smallvec::SmallVec;
use std::collections::HashMap;

mod commands;
mod projections;

pub use commands::*;
pub use projections::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GameId(i64);

impl From<i64> for GameId {
    fn from(value: i64) -> Self {
        Self(value)
    }
}

impl From<GameId> for i64 {
    fn from(value: GameId) -> Self {
        value.0
    }
}

#[derive(Debug)]
pub struct Game {
    pub id: GameId,
    pub name: String,
    pub source: GameSource,
}

#[derive(Debug)]
pub struct GameSource {
    pub source_id: String,
    pub lookup_id: String,
    pub external_ids: HashMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GameEntryId(i64);

impl From<i64> for GameEntryId {
    fn from(value: i64) -> Self {
        Self(value)
    }
}

impl From<GameEntryId> for i64 {
    fn from(value: GameEntryId) -> Self {
        value.0
    }
}

#[derive(Debug)]
pub struct GameEntry {
    pub id: GameEntryId,
    pub primary_game_id: GameId,
    pub games: SmallVec<[Game; 1]>,
}
