use crate::domain::game::GameSource;

#[derive(Debug)]
pub struct NewGame {
    pub name: String,
    pub source: GameSource,
}
