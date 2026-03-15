use crate::schema::{game_entries, games};
use diesel::{
    Selectable,
    prelude::{Associations, Identifiable, Insertable, Queryable},
    sqlite::Sqlite,
};
use domain::{Game, GameId, GameSource};
use serde_json::Value;

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
    pub external_ids: Value,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = games)]
#[diesel(check_for_backend(Sqlite))]
pub struct NewGameRow<'a> {
    pub entry_id: i32,
    pub name: &'a str,
    pub source_id: &'a str,
    pub lookup_id: &'a str,
    pub external_ids: Value,
}

impl From<GameRow> for Game {
    fn from(row: GameRow) -> Self {
        Self {
            id: GameId::from(row.id),
            name: row.name,
            source: GameSource {
                source_id: row.source_id,
                lookup_id: row.lookup_id,
                external_ids: serde_json::from_value(row.external_ids).unwrap_or_default(),
            },
        }
    }
}
