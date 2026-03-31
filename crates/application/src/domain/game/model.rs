use crate::domain::game::{GameEntryId, GameId, GameSource};
use smallvec::SmallVec;

#[derive(Debug)]
pub struct Game {
    pub id: GameId,
    pub name: String,
    pub source: GameSource,
}

#[derive(Debug)]
pub struct GameEntry {
    pub id: GameEntryId,
    pub primary_game_id: GameId,
    pub games: SmallVec<[Game; 1]>,
}
