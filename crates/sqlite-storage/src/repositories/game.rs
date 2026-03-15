use crate::SqliteStorage;
use crate::models::{GameEntryRow, GameRow, NewGameEntryRow, NewGameRow};
use anyhow::Result;
use diesel::{Connection, ExpressionMethods, QueryDsl, RunQueryDsl};
use domain::storage::GameStorage;
use domain::{Game, GameEntry, GameEntryId, GameId, NewGame};
use smallvec::SmallVec;

impl GameStorage for SqliteStorage {
    fn insert(&self, new_game: &NewGame) -> Result<GameEntry> {
        use crate::schema::game_entries::dsl as entry_dsl;
        use crate::schema::games::dsl as game_dsl;

        let mut conn = self.pool.get()?;

        conn.transaction(|conn| {
            let entry_row: GameEntryRow = diesel::insert_into(entry_dsl::game_entries)
                .values(NewGameEntryRow { primary_game_id: 0 })
                .get_result(conn)?;

            let game_row: GameRow = diesel::insert_into(game_dsl::games)
                .values(NewGameRow {
                    entry_id: entry_row.id,
                    name: &new_game.name,
                    source_id: &new_game.source.source_id,
                    lookup_id: &new_game.source.lookup_id,
                    external_ids: serde_json::to_value(&new_game.source.external_ids)?,
                })
                .get_result(conn)?;

            diesel::update(entry_dsl::game_entries.find(entry_row.id))
                .set(entry_dsl::primary_game_id.eq(game_row.id))
                .execute(conn)?;

            let game_id = GameId::from(game_row.id);
            let game = Game::from(game_row);

            Ok(GameEntry {
                id: GameEntryId::from(entry_row.id),
                primary_game: game,
                game_ids: SmallVec::from_slice(&[game_id]),
            })
        })
    }
}
