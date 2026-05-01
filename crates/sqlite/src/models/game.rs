use crate::schema::games;
use diesel::{
    Selectable,
    prelude::{Identifiable, Insertable, Queryable},
    sqlite::Sqlite,
};
use domain::game::{Game, GameSource};
use serde_json::Value;

#[derive(Queryable, Selectable, Identifiable, Debug)]
#[diesel(table_name = games)]
#[diesel(check_for_backend(Sqlite))]
pub struct GameRow {
    pub id: i64,
    pub name: String,
    pub source_id: String,
    pub lookup_id: String,
    pub external_ids: Value,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = games)]
#[diesel(check_for_backend(Sqlite))]
pub struct NewGameRow<'a> {
    pub name: &'a str,
    pub source_id: &'a str,
    pub lookup_id: &'a str,
    pub external_ids: Value,
}

impl From<GameRow> for Game {
    fn from(row: GameRow) -> Self {
        Self {
            id: (row.id as u64).into(),
            name: row.name,
            source: GameSource {
                source_id: row.source_id,
                lookup_id: row.lookup_id,
                external_ids: serde_json::from_value(row.external_ids).unwrap_or_default(),
            },
        }
    }
}
