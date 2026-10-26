use crate::id::from_row_id;
use crate::schema::downloads;
use diesel::{Selectable, prelude::*, sqlite::Sqlite};
use domain::download::Download;
use serde_json::Value;

#[derive(Queryable, Selectable, Identifiable, Debug)]
#[diesel(table_name = downloads)]
#[diesel(check_for_backend(Sqlite))]
pub struct DownloadRow {
    pub id: i64,
    pub game_id: i64,
    pub version_path: Value,
    pub target_dir: String,
    pub install_size: Option<i64>,
    pub download_size: Option<i64>,
    pub downloaded: i64,
    pub error: Option<String>,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = downloads)]
#[diesel(check_for_backend(Sqlite))]
pub struct NewDownloadRow<'a> {
    pub game_id: i64,
    pub position: i32,
    pub version_path: Value,
    pub target_dir: &'a str,
    pub install_size: Option<i64>,
    pub download_size: Option<i64>,
}

impl TryFrom<DownloadRow> for Download {
    type Error = serde_json::Error;

    fn try_from(row: DownloadRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: from_row_id(row.id),
            game_id: from_row_id(row.game_id),
            version_path: serde_json::from_value(row.version_path)?,
            target_dir: row.target_dir.into(),
            install_size: row.install_size.map(|size| size as u64),
            download_size: row.download_size.map(|size| size as u64),
            downloaded: row.downloaded as u64,
            error: row.error,
        })
    }
}
