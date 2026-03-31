use std::collections::HashMap;

#[derive(Debug)]
pub struct GameSource {
    pub source_id: String,
    pub lookup_id: String,
    pub external_ids: HashMap<String, String>,
}

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
