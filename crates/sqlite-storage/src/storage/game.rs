use crate::SqliteStorage;
use crate::models::{GameRow, NewGameRow};
use anyhow::Result;
use application::game::{GameListItem, GameQuery};
use diesel::{Connection, ExpressionMethods, QueryDsl, RunQueryDsl, SelectableHelper};
use domain::game::{GameRepository, NewGame};

impl GameRepository for SqliteStorage {
    fn insert(&self, new_game: &NewGame) -> Result<()> {
        use crate::schema::games::dsl;

        let mut conn = self.pool.get()?;
        diesel::insert_or_ignore_into(dsl::games)
            .values(NewGameRow {
                name: &new_game.name,
                source_id: &new_game.source.source_id,
                lookup_id: &new_game.source.lookup_id,
                external_ids: serde_json::to_value(&new_game.source.external_ids)?,
            })
            .execute(&mut conn)?;
        Ok(())
    }

    fn insert_many(&self, games: &[NewGame]) -> Result<()> {
        use crate::schema::games::dsl;

        let mut conn = self.pool.get()?;
        conn.transaction(|conn| {
            for game in games {
                diesel::insert_or_ignore_into(dsl::games)
                    .values(NewGameRow {
                        name: &game.name,
                        source_id: &game.source.source_id,
                        lookup_id: &game.source.lookup_id,
                        external_ids: serde_json::to_value(&game.source.external_ids)?,
                    })
                    .execute(conn)?;
            }
            Ok(())
        })
    }
}

impl GameQuery for SqliteStorage {
    fn count(&self) -> Result<usize> {
        use crate::schema::games::dsl;

        let mut conn = self.pool.get()?;
        let count = dsl::games.count().get_result::<i64>(&mut conn)?;
        Ok(count as usize)
    }

    fn list(&self, offset: usize, limit: usize) -> Result<Vec<GameListItem>> {
        use crate::schema::games::dsl;

        let mut conn = self.pool.get()?;
        let rows: Vec<GameRow> = dsl::games
            .order(dsl::name.asc())
            .limit(limit as i64)
            .offset(offset as i64)
            .select(GameRow::as_select())
            .load(&mut conn)?;
        let items = rows
            .into_iter()
            .map(|row| GameListItem {
                id: row.id.into(),
                name: row.name,
                source_id: row.source_id,
            })
            .collect();
        Ok(items)
    }
}
