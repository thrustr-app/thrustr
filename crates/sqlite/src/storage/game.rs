use crate::SqliteStorage;
use crate::id::{from_row_id, to_row_id};
use crate::models::{ArtworkRow, GameRow, NewGameRow};
use anyhow::Result;
use diesel::{
    BoolExpressionMethods, Connection, ExpressionMethods, JoinOnDsl, OptionalExtension, QueryDsl,
    RunQueryDsl, SelectableHelper, dsl::sql, sql_types::Untyped,
};
use domain::artwork::{Artwork, ArtworkKind};
use domain::game::{Game, GameId, GameIndex, GameListItem, GameRepository, NewGame, name_bucket};
use std::collections::HashMap;
use tracing::warn;

impl GameRepository for SqliteStorage {
    fn insert(&self, game: &NewGame) -> Result<Option<Game>> {
        use crate::schema::games::dsl;

        let mut conn = self.pool.get()?;
        let row = diesel::insert_or_ignore_into(dsl::games)
            .values(NewGameRow::from(game))
            .returning(GameRow::as_returning())
            .get_result::<GameRow>(&mut conn)
            .optional()?;

        Ok(row.map(Game::from))
    }

    fn insert_many(&self, games: &[NewGame]) -> Result<usize> {
        use crate::schema::games::dsl;

        const CHUNK_SIZE: usize = 1000;

        let mut conn = self.pool.get()?;
        conn.transaction(|conn| {
            let mut inserted = 0;
            for chunk in games.chunks(CHUNK_SIZE) {
                let rows: Vec<NewGameRow> = chunk.iter().map(NewGameRow::from).collect();
                inserted += diesel::insert_or_ignore_into(dsl::games)
                    .values(rows)
                    .execute(conn)?;
            }
            Ok(inserted)
        })
    }

    fn get(&self, id: GameId) -> Result<Option<Game>> {
        use crate::schema::games::dsl;

        let id = to_row_id(id);
        let mut conn = self.pool.get()?;
        let row = dsl::games
            .find(id)
            .select(GameRow::as_select())
            .first::<GameRow>(&mut conn)
            .optional()?;

        Ok(row.map(Game::from))
    }

    fn list_index(&self) -> Result<GameIndex> {
        use crate::schema::games::dsl;

        let mut conn = self.pool.get()?;
        // Ordering by column would fall back to BINARY
        // and sort every lowercase name after every uppercase one.
        let rows = dsl::games
            .order(sql::<Untyped>("name COLLATE NOCASE ASC, id ASC"))
            .select((dsl::id, dsl::name))
            .load::<(i64, String)>(&mut conn)?;

        let mut ids = Vec::with_capacity(rows.len());
        let sections = rows
            .into_iter()
            .map(|(id, name)| {
                ids.push(from_row_id(id));
                name_bucket(&name)
            })
            .collect();

        Ok(GameIndex { ids, sections })
    }

    fn list_by_ids(&self, ids: &[GameId]) -> Result<Vec<GameListItem>> {
        use crate::schema::artwork;
        use crate::schema::games::dsl;

        const CHUNK_SIZE: usize = 1000;

        let mut conn = self.pool.get()?;
        let mut by_id: HashMap<i64, GameListItem> = HashMap::with_capacity(ids.len());
        for chunk in ids.chunks(CHUNK_SIZE) {
            let row_ids: Vec<i64> = chunk.iter().map(|id| to_row_id(*id)).collect();
            let rows: Vec<(GameRow, Option<ArtworkRow>)> = dsl::games
                .left_join(
                    artwork::table.on(artwork::game_id
                        .eq(dsl::id)
                        .and(artwork::kind.eq(ArtworkKind::Cover.as_ref()))),
                )
                .filter(dsl::id.eq_any(row_ids))
                .select((GameRow::as_select(), Option::<ArtworkRow>::as_select()))
                .load(&mut conn)?;

            for (game, cover) in rows {
                by_id.insert(game.id, list_item(game, cover));
            }
        }

        Ok(ids
            .iter()
            .filter_map(|id| by_id.remove(&to_row_id(*id)))
            .collect())
    }

    fn list_missing_artwork(
        &self,
        kind: ArtworkKind,
        after: GameId,
        limit: usize,
    ) -> Result<Vec<(GameId, String)>> {
        use crate::schema::artwork;
        use crate::schema::games::dsl;

        let after = to_row_id(after);
        let mut conn = self.pool.get()?;
        let rows: Vec<(i64, Option<String>)> = dsl::games
            .left_join(
                artwork::table.on(artwork::game_id
                    .eq(dsl::id)
                    .and(artwork::kind.eq(kind.as_ref()))),
            )
            // TODO: when per-kind source URLs exist, filter on that
            .filter(dsl::cover_url.is_not_null())
            .filter(artwork::game_id.is_null())
            .filter(dsl::id.gt(after))
            .order(dsl::id.asc())
            .limit(limit as i64)
            .select((dsl::id, dsl::cover_url))
            .load(&mut conn)?;

        Ok(rows
            .into_iter()
            .filter_map(|(id, url)| url.map(|url| (from_row_id(id), url)))
            .collect())
    }
}

fn list_item(game: GameRow, cover: Option<ArtworkRow>) -> GameListItem {
    GameListItem {
        id: from_row_id(game.id),
        name: game.name,
        source_id: game.source_id,
        cover_url: game.cover_url,
        cover: cover.and_then(|row| {
            Artwork::try_from(row)
                .inspect_err(|err| warn!(game_id = game.id, "skipping artwork row: {err}"))
                .ok()
        }),
    }
}
