use crate::SqliteStorage;
use crate::id::{from_row_id, to_row_id};
use crate::models::{ArtworkRow, GameRow, NewGameRow};
use crate::schema::{artwork, games};
use anyhow::Result;
use diesel::{
    BoolExpressionMethods, ExpressionMethods, JoinOnDsl, NullableExpressionMethods,
    OptionalExtension, QueryDsl, QueryResult, RunQueryDsl, SelectableHelper, SqliteConnection,
    dsl::{max, not},
    sql_types::{BigInt, Bool, Text},
};
use domain::artwork::{Artwork, ArtworkKind};
use domain::game::{
    self, Game, GameId, GameIndex, GameListItem, GameRepository, NewGame, SyncSummary,
};
use std::collections::HashMap;
use tracing::warn;

mod search;

const CHUNK_SIZE: usize = 1000;

impl GameRepository for SqliteStorage {
    fn insert(&self, game: &NewGame) -> Result<Option<Game>> {
        let row = NewGameRow::from(game);

        self.write(|conn| {
            let inserted = diesel::insert_or_ignore_into(games::table)
                .values(&row)
                .returning(GameRow::as_returning())
                .get_result::<GameRow>(conn)
                .optional()?;

            if inserted.is_some() {
                search::index_vocab(conn, search::vocab_words([row.sort_name.as_str()]))?;
            }

            Ok(inserted.map(Game::from))
        })
    }

    fn sync(&self, source_id: &str, games: &[NewGame]) -> Result<SyncSummary> {
        debug_assert!(games.iter().all(|game| game.source.id == source_id));

        let mut summary = SyncSummary::default();

        for chunk in games.chunks(CHUNK_SIZE) {
            let rows: Vec<NewGameRow> = chunk.iter().map(NewGameRow::from).collect();
            let (inserted, updated) = self.write(|conn| upsert(conn, source_id, &rows))?;
            summary.inserted += inserted;
            summary.updated += updated;
        }

        let lookup_ids: Vec<&str> = games
            .iter()
            .map(|game| game.source.lookup_id.as_str())
            .collect();
        let lookup_ids = serde_json::to_string(&lookup_ids)?;
        let changed = self.write(|conn| Ok(update_removed(conn, source_id, &lookup_ids)?))?;
        summary.removed = changed.iter().filter(|&&removed| removed).count();
        summary.restored = changed.len() - summary.removed;

        Ok(summary)
    }

    fn get(&self, id: GameId) -> Result<Option<Game>> {
        let row: Option<(GameRow, Option<ArtworkRow>)> = self.read(|conn| {
            Ok(visible_with_artwork(ArtworkKind::Cover)
                .filter(games::id.eq(to_row_id(id)))
                .select((GameRow::as_select(), Option::<ArtworkRow>::as_select()))
                .first(conn)
                .optional()?)
        })?;

        Ok(row.map(|(row, cover_row)| Game {
            cover: cover(row.id, cover_row),
            ..Game::from(row)
        }))
    }

    fn list_index(&self, query: Option<&str>) -> Result<GameIndex> {
        let Some(query) = query.map(str::trim).filter(|query| !query.is_empty()) else {
            return self.read(browse);
        };

        let normalized = game::normalize(query);
        if normalized.is_empty() {
            return Ok(GameIndex::default());
        }

        let ids = self.read(|conn| search::search(conn, &normalized))?;
        Ok(GameIndex::from_ids(ids))
    }

    fn list_by_ids(&self, ids: &[GameId]) -> Result<Vec<GameListItem>> {
        let mut by_id: HashMap<i64, GameListItem> = HashMap::with_capacity(ids.len());
        self.read(|conn| {
            for chunk in ids.chunks(CHUNK_SIZE) {
                let row_ids: Vec<i64> = chunk.iter().map(|&id| to_row_id(id)).collect();
                let rows: Vec<(GameRow, Option<ArtworkRow>)> =
                    visible_with_artwork(ArtworkKind::Cover)
                        .filter(games::id.eq_any(row_ids))
                        .select((GameRow::as_select(), Option::<ArtworkRow>::as_select()))
                        .load(conn)?;

                for (game, cover) in rows {
                    by_id.insert(game.id, list_item(game, cover));
                }
            }
            Ok(())
        })?;

        Ok(ids
            .iter()
            .filter_map(|&id| by_id.remove(&to_row_id(id)))
            .collect())
    }

    fn list_missing_artwork(
        &self,
        kind: ArtworkKind,
        after: GameId,
        limit: usize,
    ) -> Result<Vec<(GameId, String)>> {
        let rows: Vec<(i64, String)> = self.read(|conn| {
            Ok(visible_with_artwork(kind)
                // TODO: when per-kind source URLs exist, filter on that
                .filter(games::cover_url.is_not_null())
                .filter(artwork::game_id.is_null())
                .filter(games::id.gt(to_row_id(after)))
                .order(games::id.asc())
                .limit(limit as i64)
                .select((games::id, games::cover_url.assume_not_null()))
                .load(conn)?)
        })?;

        Ok(rows
            .into_iter()
            .map(|(id, url)| (from_row_id(id), url))
            .collect())
    }
}

/// Inserts new games and updates existing ones.
/// Returns the number of inserted and updated games.
fn upsert(
    conn: &mut SqliteConnection,
    source_id: &str,
    rows: &[NewGameRow],
) -> Result<(usize, usize)> {
    #[derive(diesel::QueryableByName)]
    struct ChangedRow {
        #[diesel(sql_type = BigInt)]
        id: i64,
        #[diesel(sql_type = Text)]
        sort_name: String,
    }

    // One JSON parameter is faster than binding every field.
    let json: Vec<_> = rows
        .iter()
        .map(|row| {
            (
                row.lookup_id,
                row.name,
                &row.sort_name,
                &row.search_text,
                row.external_ids.to_string(),
                row.cover_url,
                row.summary,
                row.description,
            )
        })
        .collect();

    // Used to tell apart inserts from updates as inserts get ids above the current max.
    let max_id: Option<i64> = games::table.select(max(games::id)).first(conn)?;

    let changed = diesel::sql_query(
        "INSERT INTO games (source_id, lookup_id, name, sort_name, search_text, \
                            external_ids, cover_url, summary, description) \
         SELECT ?, value ->> 0, value ->> 1, value ->> 2, value ->> 3, \
                value ->> 4, value ->> 5, value ->> 6, value ->> 7 \
         FROM json_each(?) \
         WHERE TRUE \
         ON CONFLICT (source_id, lookup_id) DO UPDATE \
         SET name = excluded.name, \
             sort_name = excluded.sort_name, \
             search_text = excluded.search_text, \
             external_ids = excluded.external_ids, \
             cover_url = excluded.cover_url, \
             summary = excluded.summary, \
             description = excluded.description \
         WHERE name IS NOT excluded.name \
            OR sort_name IS NOT excluded.sort_name \
            OR search_text IS NOT excluded.search_text \
            OR external_ids IS NOT excluded.external_ids \
            OR cover_url IS NOT excluded.cover_url \
            OR summary IS NOT excluded.summary \
            OR description IS NOT excluded.description \
         RETURNING id, sort_name",
    )
    .bind::<Text, _>(source_id)
    .bind::<Text, _>(serde_json::to_string(&json)?)
    .load::<ChangedRow>(conn)?;

    if !changed.is_empty() {
        let words = search::vocab_words(changed.iter().map(|row| row.sort_name.as_str()));
        search::index_vocab(conn, words)?;
    }

    let inserted = changed.iter().filter(|row| Some(row.id) > max_id).count();
    Ok((inserted, changed.len() - inserted))
}

fn update_removed(
    conn: &mut SqliteConnection,
    source_id: &str,
    lookup_ids: &str,
) -> QueryResult<Vec<bool>> {
    #[derive(diesel::QueryableByName)]
    struct ChangedRow {
        #[diesel(sql_type = Bool)]
        removed: bool,
    }

    let rows = diesel::sql_query(
        "UPDATE games \
         SET removed = NOT removed \
         WHERE source_id = ? \
           AND removed = (lookup_id IN (SELECT value FROM json_each(?))) \
         RETURNING removed",
    )
    .bind::<Text, _>(source_id)
    .bind::<Text, _>(lookup_ids)
    .load::<ChangedRow>(conn)?;

    Ok(rows.into_iter().map(|row| row.removed).collect())
}

fn browse(conn: &mut SqliteConnection) -> Result<GameIndex> {
    #[derive(diesel::QueryableByName)]
    struct BrowseRow {
        #[diesel(sql_type = BigInt)]
        id: i64,
        #[diesel(sql_type = Text)]
        sort_name: String,
    }

    let rows = diesel::sql_query(
        "SELECT id, sort_name \
         FROM games \
         WHERE removed = FALSE \
         ORDER BY (sort_name GLOB '[a-z]*'), sort_name, id",
    )
    .load::<BrowseRow>(conn)?;

    Ok(GameIndex::from_sorted(
        rows.into_iter()
            .map(|row| (from_row_id(row.id), row.sort_name)),
    ))
}

fn list_item(game: GameRow, cover_row: Option<ArtworkRow>) -> GameListItem {
    GameListItem {
        id: from_row_id(game.id),
        name: game.name,
        source_id: game.source_id,
        cover_url: game.cover_url,
        cover: cover(game.id, cover_row),
    }
}

fn cover(game_id: i64, row: Option<ArtworkRow>) -> Option<Artwork> {
    row.and_then(|row| {
        Artwork::try_from(row)
            .inspect_err(|err| warn!(game_id, "skipping artwork row: {err}"))
            .ok()
    })
}

#[diesel::dsl::auto_type]
fn visible_with_artwork(kind: ArtworkKind) -> _ {
    let kind: &'static str = kind.into();
    games::table
        .left_join(artwork::table.on(artwork::game_id.eq(games::id).and(artwork::kind.eq(kind))))
        .filter(not(games::removed))
}
