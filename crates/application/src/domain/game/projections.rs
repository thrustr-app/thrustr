use crate::domain::game::{GameEntryId, GameId};

#[derive(Debug)]
pub struct GameListEntry {
    pub id: GameEntryId,
    pub primary_game_id: GameId,
    pub name: String,
    pub source_id: String,
}
