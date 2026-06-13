use crate::schema::artwork;
use diesel::{Selectable, prelude::*, sqlite::Sqlite};
use domain::artwork::{Artwork, ArtworkKind, Color};
use std::str::FromStr;

#[derive(Queryable, Selectable, Identifiable, Insertable, Debug)]
#[diesel(table_name = artwork)]
#[diesel(primary_key(game_id, kind, position))]
#[diesel(check_for_backend(Sqlite))]
pub struct ArtworkRow {
    pub game_id: i64,
    pub hash: String,
    pub kind: String,
    pub position: i32,
    pub accent_color: Option<i32>,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = artwork)]
#[diesel(primary_key(game_id, kind, position))]
#[diesel(check_for_backend(Sqlite))]
pub struct NewArtworkRow<'a> {
    pub game_id: i64,
    pub hash: &'a str,
    pub kind: &'a str,
    pub position: i32,
    pub accent_color: Option<i32>,
}

impl From<ArtworkRow> for Artwork {
    fn from(row: ArtworkRow) -> Self {
        Self {
            hash: row.hash,
            kind: ArtworkKind::from_str(&row.kind).unwrap(),
            position: row.position as u32,
            accent_color: row.accent_color.map(|c| Color::from_hex(c as u32)),
        }
    }
}
