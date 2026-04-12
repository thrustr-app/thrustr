use crate::{game::Game, id::Id};

#[derive(Debug)]
pub struct GameListItem {
    pub id: Id<Game>,
    pub name: String,
    pub source_id: String,
}
