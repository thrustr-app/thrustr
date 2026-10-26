use crate::{
    SqliteStorage,
    id::to_row_id,
    models::{DownloadRow, NewDownloadRow},
};
use anyhow::{Result, anyhow};
use diesel::{
    ExpressionMethods, IntoSql, QueryDsl, RunQueryDsl, SelectableHelper,
    dsl::{count_star, max},
    sql_types::Integer,
};
use domain::download::{Download, DownloadId, DownloadRepository, NewDownload};
use std::path::Path;

impl DownloadRepository for SqliteStorage {
    fn enqueue(&self, download: &NewDownload) -> Result<Download> {
        use crate::schema::downloads::dsl;

        let target_dir = path_to_str(&download.target_dir)?;
        let version_path = serde_json::to_value(&download.version_path)?;

        let row = self.write(|conn| {
            let position = dsl::downloads
                .select(max(dsl::position))
                .first::<Option<i32>>(conn)?
                .map_or(0, |last| last + 1);

            Ok(diesel::insert_into(dsl::downloads)
                .values(NewDownloadRow {
                    game_id: to_row_id(download.game_id),
                    position,
                    version_path,
                    target_dir,
                    install_size: download.install_size.map(|size| size as i64),
                    download_size: download.download_size.map(|size| size as i64),
                })
                .returning(DownloadRow::as_returning())
                .get_result::<DownloadRow>(conn)?)
        })?;

        Ok(row.try_into()?)
    }

    fn list(&self) -> Result<Vec<Download>> {
        use crate::schema::downloads::dsl;

        let rows = self.read(|conn| {
            Ok(dsl::downloads
                .order(dsl::position)
                .select(DownloadRow::as_select())
                .load::<DownloadRow>(conn)?)
        })?;

        Ok(rows
            .into_iter()
            .map(Download::try_from)
            .collect::<Result<_, _>>()?)
    }

    fn save(&self, download: &Download) -> Result<()> {
        use crate::schema::downloads::dsl;

        let target_dir = path_to_str(&download.target_dir)?;
        let version_path = serde_json::to_value(&download.version_path)?;

        self.write(|conn| {
            diesel::update(dsl::downloads.find(to_row_id(download.id)))
                .set((
                    dsl::version_path.eq(version_path),
                    dsl::target_dir.eq(target_dir),
                    dsl::install_size.eq(download.install_size.map(|size| size as i64)),
                    dsl::download_size.eq(download.download_size.map(|size| size as i64)),
                    dsl::downloaded.eq(download.downloaded as i64),
                    dsl::error.eq(download.error.as_deref()),
                ))
                .execute(conn)?;
            Ok(())
        })
    }

    fn move_to(&self, id: DownloadId, position: u32) -> Result<()> {
        use crate::schema::downloads::dsl;

        let id = to_row_id(id);

        self.write(|conn| {
            let from = dsl::downloads
                .find(id)
                .select(dsl::position)
                .first::<i32>(conn)?;
            let last = dsl::downloads.select(count_star()).first::<i64>(conn)? as i32 - 1;
            let to = (position.min(i32::MAX as u32) as i32).min(last);

            if from == to {
                return Ok(());
            }

            let (range, shift) = if to < from {
                (to..=from - 1, 1)
            } else {
                (from + 1..=to, -1)
            };

            diesel::update(
                dsl::downloads.filter(dsl::position.between(*range.start(), *range.end())),
            )
            .set(dsl::position.eq((-1 - shift).into_sql::<Integer>() - dsl::position))
            .execute(conn)?;
            diesel::update(dsl::downloads.find(id))
                .set(dsl::position.eq(to))
                .execute(conn)?;
            diesel::update(dsl::downloads.filter(dsl::position.lt(0)))
                .set(dsl::position.eq((-1).into_sql::<Integer>() - dsl::position))
                .execute(conn)?;
            Ok(())
        })
    }

    fn remove(&self, id: DownloadId) -> Result<()> {
        use crate::schema::downloads::dsl;

        self.write(|conn| {
            diesel::delete(dsl::downloads.find(to_row_id(id))).execute(conn)?;
            Ok(())
        })
    }
}

fn path_to_str(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| anyhow!("non UTF-8 download target dir"))
}
