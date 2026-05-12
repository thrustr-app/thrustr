use crate::SqliteStorage;
use crate::models::{GameRow, NewGameRow};
use anyhow::Result;
use diesel::{
    Connection, ExpressionMethods, OptionalExtension, QueryDsl, RunQueryDsl, SelectableHelper,
};
use domain::game::{Game, GameRepository, NewGame};
use game::{GameListItem, GameQuery};

impl GameRepository for SqliteStorage {
    fn insert(&self, new_game: &NewGame) -> Result<Option<Game>> {
        use crate::schema::games::dsl;

        let mut conn = self.pool.get()?;
        let row = diesel::insert_or_ignore_into(dsl::games)
            .values(NewGameRow {
                name: &new_game.name,
                source_id: &new_game.source.source_id,
                lookup_id: &new_game.source.lookup_id,
                external_ids: serde_json::to_value(&new_game.source.external_ids)?,
                cover_url: &new_game.cover_url,
            })
            .returning(GameRow::as_returning())
            .get_result::<GameRow>(&mut conn)
            .optional()?;

        Ok(row.map(Game::from))
    }

    fn insert_many(&self, games: &[NewGame]) -> Result<Vec<Game>> {
        use crate::schema::games::dsl;

        let mut conn = self.pool.get()?;
        conn.transaction(|conn| {
            let mut inserted = Vec::new();
            for game in games {
                let row = diesel::insert_or_ignore_into(dsl::games)
                    .values(NewGameRow {
                        name: &game.name,
                        source_id: &game.source.source_id,
                        lookup_id: &game.source.lookup_id,
                        external_ids: serde_json::to_value(&game.source.external_ids)?,
                        cover_url: &game.cover_url,
                    })
                    .returning(GameRow::as_returning())
                    .get_result::<GameRow>(conn)
                    .optional()?;

                if let Some(row) = row {
                    inserted.push(Game::from(row));
                }
            }
            Ok(inserted)
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
                id: (row.id as u64).into(),
                name: row.name,
                source_id: row.source_id,
            })
            .collect();
        Ok(items)
    }
}
