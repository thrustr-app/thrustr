use smallvec::SmallVec;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GameId(i32);

impl From<i32> for GameId {
    fn from(value: i32) -> Self {
        Self(value)
    }
}

impl From<GameId> for i32 {
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
pub struct GameEntryId(i32);

impl From<i32> for GameEntryId {
    fn from(value: i32) -> Self {
        Self(value)
    }
}

impl From<GameEntryId> for i32 {
    fn from(value: GameEntryId) -> Self {
        value.0
    }
}

#[derive(Debug)]
pub struct GameEntry {
    pub id: GameEntryId,
    pub primary_game: Game,
    pub game_ids: SmallVec<[GameId; 1]>,
}

#[derive(Debug)]
pub struct NewGame {
    pub name: String,
    pub source: GameSource,
}
