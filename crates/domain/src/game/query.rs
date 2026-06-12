use crate::{artwork::Artwork, game::GameId};

#[derive(Debug)]
pub struct GameListItem {
    pub id: GameId,
    pub name: String,
    pub source_id: String,
    pub artwork: Option<Artwork>,
}
