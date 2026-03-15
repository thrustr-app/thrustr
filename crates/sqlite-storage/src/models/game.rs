use crate::schema::{game_entries, game_external_ids, games};
use diesel::{
    Selectable,
    prelude::{Associations, Identifiable, Insertable, Queryable},
    sqlite::Sqlite,
};
use domain::{Game, GameId, GameSource};
use std::collections::HashMap;

#[derive(Queryable, Selectable, Identifiable, Debug)]
#[diesel(table_name = game_entries)]
#[diesel(check_for_backend(Sqlite))]
pub struct GameEntryRow {
    pub id: i32,
    pub primary_game_id: i32,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = game_entries)]
#[diesel(check_for_backend(Sqlite))]
pub struct NewGameEntryRow {
    pub primary_game_id: i32,
}

#[derive(Queryable, Selectable, Identifiable, Associations, Debug)]
#[diesel(table_name = games)]
#[diesel(belongs_to(GameEntryRow, foreign_key = entry_id))]
#[diesel(check_for_backend(Sqlite))]
pub struct GameRow {
    pub id: i32,
    pub entry_id: i32,
    pub name: String,
    pub source_id: String,
    pub lookup_id: String,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = games)]
#[diesel(check_for_backend(Sqlite))]
pub struct NewGameRow<'a> {
    pub entry_id: i32,
    pub name: &'a str,
    pub source_id: &'a str,
    pub lookup_id: &'a str,
}

#[derive(Queryable, Selectable, Identifiable, Associations, Debug)]
#[diesel(table_name = game_external_ids)]
#[diesel(primary_key(game_id, key))]
#[diesel(belongs_to(GameRow, foreign_key = game_id))]
#[diesel(check_for_backend(Sqlite))]
pub struct GameExternalIdRow {
    pub game_id: i32,
    pub key: String,
    pub value: String,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = game_external_ids)]
#[diesel(check_for_backend(Sqlite))]
pub struct NewGameExternalIdRow<'a> {
    pub game_id: i32,
    pub key: &'a str,
    pub value: &'a str,
}

impl GameRow {
    pub fn to_game(self, ext_ids: Vec<GameExternalIdRow>) -> Game {
        Game {
            id: GameId::from(self.id),
            name: self.name,
            source: GameSource {
                source_id: self.source_id,
                lookup_id: self.lookup_id,
                external_ids: ext_ids
                    .into_iter()
                    .map(|e| (e.key, e.value))
                    .collect::<HashMap<_, _>>(),
            },
        }
    }
}
