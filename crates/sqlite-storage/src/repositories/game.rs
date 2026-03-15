use crate::SqliteStorage;
use crate::models::{
    GameEntryRow, GameExternalIdRow, GameRow, NewGameEntryRow, NewGameExternalIdRow, NewGameRow,
};
use anyhow::Result;
use diesel::{BelongingToDsl, Connection, ExpressionMethods, QueryDsl, RunQueryDsl};
use domain::storage::GameStorage;
use domain::{GameEntry, GameEntryId, GameId, NewGame};
use smallvec::SmallVec;

impl GameStorage for SqliteStorage {
    fn insert(&self, new_game: &NewGame) -> Result<GameEntry> {
        use crate::schema::game_entries::dsl as entry_dsl;
        use crate::schema::game_external_ids::dsl as ext_dsl;
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
                })
                .get_result(conn)?;

            let ext_rows: Vec<NewGameExternalIdRow> = new_game
                .source
                .external_ids
                .iter()
                .map(|(k, v)| NewGameExternalIdRow {
                    game_id: game_row.id,
                    key: k,
                    value: v,
                })
                .collect();
            if !ext_rows.is_empty() {
                diesel::insert_into(ext_dsl::game_external_ids)
                    .values(&ext_rows)
                    .execute(conn)?;
            }

            diesel::update(entry_dsl::game_entries.find(entry_row.id))
                .set(entry_dsl::primary_game_id.eq(game_row.id))
                .execute(conn)?;

            let ext_ids =
                GameExternalIdRow::belonging_to(&game_row).load::<GameExternalIdRow>(conn)?;
            let game_id = GameId::from(game_row.id);
            let game = game_row.to_game(ext_ids);

            Ok(GameEntry {
                id: GameEntryId::from(entry_row.id),
                primary_game: game,
                game_ids: SmallVec::from_slice(&[game_id]),
            })
        })
    }
}
