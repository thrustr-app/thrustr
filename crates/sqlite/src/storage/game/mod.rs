use crate::SqliteStorage;
use crate::id::{from_row_id, to_row_id};
use crate::models::{ArtworkRow, GameRow, NewGameRow};
use crate::schema::{artwork, games};
use anyhow::Result;
use diesel::{
    BoolExpressionMethods, Connection, ExpressionMethods, JoinOnDsl, OptionalExtension, QueryDsl,
    QueryResult, RunQueryDsl, SelectableHelper, SqliteConnection,
    dsl::not,
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
        use crate::schema::games::dsl;

        let mut conn = self.conn()?;
        conn.transaction(|conn| {
            let row = NewGameRow::from(game);
            let words = search::vocab_words([row.sort_name.as_str()]);

            let inserted = diesel::insert_or_ignore_into(dsl::games)
                .values(row)
                .returning(GameRow::as_returning())
                .get_result::<GameRow>(conn)
                .optional()?;

            if inserted.is_some() {
                search::index_vocab(conn, words)?;
            }

            Ok(inserted.map(Game::from))
        })
    }

    fn sync(&self, source_id: &str, games: &[NewGame]) -> Result<SyncSummary> {
        debug_assert!(games.iter().all(|game| game.source.id == source_id));

        let lookup_ids: Vec<&str> = games
            .iter()
            .map(|game| game.source.lookup_id.as_str())
            .collect();
        let lookup_ids = serde_json::to_string(&lookup_ids)?;

        let mut conn = self.conn()?;
        conn.transaction(|conn| {
            let inserted = insert_new(conn, games)?;
            let changed = update_removed(conn, source_id, &lookup_ids)?;
            let removed = changed.iter().filter(|&&removed| removed).count();

            Ok(SyncSummary {
                inserted,
                restored: changed.len() - removed,
                removed,
            })
        })
    }

    fn get(&self, id: GameId) -> Result<Option<Game>> {
        use crate::schema::games::dsl;

        let id = to_row_id(id);
        let mut conn = self.conn()?;
        let row: Option<(GameRow, Option<ArtworkRow>)> = visible_with_cover()
            .filter(dsl::id.eq(id))
            .select((GameRow::as_select(), Option::<ArtworkRow>::as_select()))
            .first(&mut conn)
            .optional()?;

        Ok(row.map(|(row, cover_row)| Game {
            cover: cover(row.id, cover_row),
            ..Game::from(row)
        }))
    }

    fn list_index(&self, query: Option<&str>) -> Result<GameIndex> {
        let mut conn = self.conn()?;

        let Some(query) = query.map(str::trim).filter(|query| !query.is_empty()) else {
            return browse(&mut conn);
        };

        let normalized = game::normalize(query);
        if normalized.is_empty() {
            return Ok(GameIndex::default());
        }

        let ids = search::search(&mut conn, &normalized)?;
        Ok(GameIndex::from_ids(ids))
    }

    fn list_by_ids(&self, ids: &[GameId]) -> Result<Vec<GameListItem>> {
        use crate::schema::games::dsl;

        let mut conn = self.conn()?;
        let mut by_id: HashMap<i64, GameListItem> = HashMap::with_capacity(ids.len());
        for chunk in ids.chunks(CHUNK_SIZE) {
            let row_ids: Vec<i64> = chunk.iter().map(|&id| to_row_id(id)).collect();
            let rows: Vec<(GameRow, Option<ArtworkRow>)> = visible_with_cover()
                .filter(dsl::id.eq_any(row_ids))
                .select((GameRow::as_select(), Option::<ArtworkRow>::as_select()))
                .load(&mut conn)?;

            for (game, cover) in rows {
                by_id.insert(game.id, list_item(game, cover));
            }
        }

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
        use crate::schema::games::dsl;

        let after = to_row_id(after);
        let mut conn = self.conn()?;
        let rows: Vec<(i64, Option<String>)> = dsl::games
            .left_join(
                artwork::table.on(artwork::game_id
                    .eq(dsl::id)
                    .and(artwork::kind.eq(kind.as_ref()))),
            )
            .filter(not(dsl::removed))
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

fn insert_new(conn: &mut SqliteConnection, games: &[NewGame]) -> QueryResult<usize> {
    let mut inserted = 0;
    for chunk in games.chunks(CHUNK_SIZE) {
        let rows: Vec<NewGameRow> = chunk.iter().map(NewGameRow::from).collect();

        // Extract words before insert consumes them to avoid folding them again.
        // For ignored/duplicate rows, we don't check if words are already in the vocabulary
        // as identifying them is more expensive than just inserting-or-ignoring them.
        let words = search::vocab_words(rows.iter().map(|row| row.sort_name.as_str()));

        let added = diesel::insert_or_ignore_into(games::table)
            .values(rows)
            .execute(conn)?;
        inserted += added;

        if added > 0 {
            search::index_vocab(conn, words)?;
        }
    }
    Ok(inserted)
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
fn visible_with_cover() -> _ {
    let cover: &'static str = ArtworkKind::Cover.as_ref();
    games::table
        .left_join(artwork::table.on(artwork::game_id.eq(games::id).and(artwork::kind.eq(cover))))
        .filter(not(games::removed))
}
