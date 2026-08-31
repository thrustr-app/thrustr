use crate::id::from_row_id;
use crate::schema::games;
use diesel::{
    Selectable,
    prelude::{Identifiable, Insertable, Queryable},
    sqlite::Sqlite,
};
use domain::game::{Game, GameExt, GameSource, NewGame};
use serde_json::Value;
use tracing::warn;

#[derive(Queryable, Selectable, Identifiable, Debug)]
#[diesel(table_name = games)]
#[diesel(check_for_backend(Sqlite))]
pub struct GameRow {
    pub id: i64,
    pub name: String,
    pub source_id: String,
    pub lookup_id: String,
    pub external_ids: Value,
    pub cover_url: Option<String>,
    pub summary: Option<String>,
    pub description: Option<String>,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = games)]
#[diesel(check_for_backend(Sqlite))]
pub struct NewGameRow<'a> {
    pub name: &'a str,
    pub sort_name: String,
    pub source_id: &'a str,
    pub lookup_id: &'a str,
    pub external_ids: Value,
    pub cover_url: Option<&'a str>,
    pub summary: Option<&'a str>,
    pub description: Option<&'a str>,
}

impl From<GameRow> for Game {
    fn from(row: GameRow) -> Self {
        let id = from_row_id(row.id);
        let external_ids = serde_json::from_value(row.external_ids)
            .inspect_err(|err| warn!(game_id = row.id, "skipping invalid external_ids: {err}"))
            .unwrap_or_default();

        Self {
            id,
            name: row.name,
            source: GameSource {
                id: row.source_id,
                lookup_id: row.lookup_id,
                external_ids,
            },
            cover_url: row.cover_url,
            summary: row.summary,
            description: row.description,
            cover: None,
        }
    }
}

impl<'a> From<&'a NewGame> for NewGameRow<'a> {
    fn from(game: &'a NewGame) -> Self {
        Self {
            name: &game.name,
            sort_name: game.sort_name(),
            source_id: &game.source.id,
            lookup_id: &game.source.lookup_id,
            external_ids: serde_json::to_value(&game.source.external_ids).unwrap_or_default(),
            cover_url: game.cover_url.as_deref(),
            summary: game.summary.as_deref(),
            description: game.description.as_deref(),
        }
    }
}
