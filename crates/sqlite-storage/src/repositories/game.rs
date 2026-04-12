use crate::SqliteStorage;
use crate::models::{GameEntryRow, GameRow, NewGameEntryRow, NewGameRow};
use anyhow::Result;
use diesel::{
    Connection, ExpressionMethods, JoinOnDsl, OptionalExtension, QueryDsl, RunQueryDsl,
    SelectableHelper, SqliteConnection,
};
use domain::game::{GameListItem, GameRepository, NewGame};

impl GameRepository for SqliteStorage {
    fn insert(&self, new_game: &NewGame) -> Result<()> {
        let mut conn = self.pool.get()?;
        conn.transaction(|conn| self.insert_within(conn, new_game))
    }

    fn insert_many(&self, games: &[NewGame]) -> Result<()> {
        let mut conn = self.pool.get()?;
        conn.transaction(|conn| games.iter().try_for_each(|g| self.insert_within(conn, g)))
    }

    fn count(&self) -> Result<usize> {
        use crate::schema::game_entries::dsl as entry_dsl;

        let mut conn = self.pool.get()?;

        let count = entry_dsl::game_entries
            .count()
            .get_result::<i64>(&mut conn)?;
        Ok(count as usize)
    }

    fn list(&self, offset: usize, limit: usize) -> Result<Vec<GameListItem>> {
        use crate::schema::game_entries::dsl as entry_dsl;
        use crate::schema::games::dsl as game_dsl;

        let mut conn = self.pool.get()?;

        let rows: Vec<(GameEntryRow, GameRow)> = entry_dsl::game_entries
            .inner_join(game_dsl::games.on(game_dsl::id.eq(entry_dsl::primary_game_id)))
            .order(game_dsl::name.asc())
            .limit(limit as i64)
            .offset(offset as i64)
            .select((GameEntryRow::as_select(), GameRow::as_select()))
            .load(&mut conn)?;

        let entries = rows
            .into_iter()
            .map(|(entry_row, game_row)| GameListItem {
                id: entry_row.id.into(),
                name: game_row.name,
                source_id: game_row.source_id,
            })
            .collect();

        Ok(entries)
    }
}

impl SqliteStorage {
    fn insert_within(&self, conn: &mut SqliteConnection, new_game: &NewGame) -> Result<()> {
        use crate::schema::game_entries::dsl as entry_dsl;
        use crate::schema::games::dsl as game_dsl;

        let game_row: Option<GameRow> = diesel::insert_or_ignore_into(game_dsl::games)
            .values(NewGameRow {
                entry_id: 0,
                name: &new_game.name,
                source_id: &new_game.source.source_id,
                lookup_id: &new_game.source.lookup_id,
                external_ids: serde_json::to_value(&new_game.source.external_ids)?,
            })
            .get_result(conn)
            .optional()?;

        let Some(game_row) = game_row else {
            return Ok(());
        };

        let entry_row: GameEntryRow = diesel::insert_into(entry_dsl::game_entries)
            .values(NewGameEntryRow { primary_game_id: 0 })
            .get_result(conn)?;

        diesel::update(game_dsl::games.find(game_row.id))
            .set(game_dsl::entry_id.eq(entry_row.id))
            .execute(conn)?;

        diesel::update(entry_dsl::game_entries.find(entry_row.id))
            .set(entry_dsl::primary_game_id.eq(game_row.id))
            .execute(conn)?;

        Ok(())
    }
}
