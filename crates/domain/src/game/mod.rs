use smallvec::SmallVec;

mod commands;
mod projections;
mod repository;
mod value_objects;

pub use commands::*;
pub use projections::*;
pub use repository::*;
pub use value_objects::*;

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
