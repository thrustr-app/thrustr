use crate::SqliteStorage;
use crate::id::{from_row_id, to_row_id};
use crate::models::{ArtworkRow, GameRow, NewGameRow};
use anyhow::Result;
use diesel::{
    BoolExpressionMethods, Connection, ExpressionMethods, JoinOnDsl, OptionalExtension, QueryDsl,
    QueryableByName, RunQueryDsl, SelectableHelper, SqliteConnection, sql_query,
    sql_types::{BigInt, Text},
};
use domain::artwork::{Artwork, ArtworkKind};
use domain::game::{self, Game, GameExt, GameId, GameIndex, GameListItem, GameRepository, NewGame};
use std::collections::{BTreeSet, HashMap};
use tracing::{debug, warn};

const CHUNK_SIZE: usize = 1000;
const VOCAB_CHUNK_SIZE: usize = 500;

impl GameRepository for SqliteStorage {
    fn insert(&self, game: &NewGame) -> Result<Option<Game>> {
        use crate::schema::games::dsl;

        let mut conn = self.conn()?;
        conn.transaction(|conn| {
            let row = diesel::insert_or_ignore_into(dsl::games)
                .values(NewGameRow::from(game))
                .returning(GameRow::as_returning())
                .get_result::<GameRow>(conn)
                .optional()?;

            if row.is_some() {
                index_vocab(conn, [game.sort_name()])?;
            }

            Ok(row.map(Game::from))
        })
    }

    fn insert_many(&self, games: &[NewGame]) -> Result<usize> {
        use crate::schema::games::dsl;

        let mut conn = self.conn()?;
        conn.transaction(|conn| {
            let mut inserted = 0;
            for chunk in games.chunks(CHUNK_SIZE) {
                let rows: Vec<NewGameRow> = chunk.iter().map(NewGameRow::from).collect();
                let added = diesel::insert_or_ignore_into(dsl::games)
                    .values(rows)
                    .execute(conn)?;
                inserted += added;

                if added > 0 {
                    index_vocab(conn, chunk.iter().map(|g| g.sort_name()))?;
                }
            }
            Ok(inserted)
        })
    }

    fn get(&self, id: GameId) -> Result<Option<Game>> {
        use crate::schema::games::dsl;

        let id = to_row_id(id);
        let mut conn = self.conn()?;
        let row = dsl::games
            .find(id)
            .select(GameRow::as_select())
            .first::<GameRow>(&mut conn)
            .optional()?;

        Ok(row.map(Game::from))
    }

    fn list_index(&self, query: Option<&str>) -> Result<GameIndex> {
        use crate::schema::games::dsl;

        let mut conn = self.conn()?;

        if let Some(normalized) = query.map(game::normalize).filter(|q| !q.is_empty()) {
            let search = search_games(&mut conn, &normalized)?;
            debug!(
                used_spellfix = search.used_spellfix,
                hits = search.ids.len(),
                "library search"
            );
            return Ok(GameIndex::from_ids(search.ids));
        }

        let rows = dsl::games
            .order((dsl::sort_name.asc(), dsl::id.asc()))
            .select((dsl::id, dsl::sort_name))
            .load::<(i64, String)>(&mut conn)?;

        Ok(GameIndex::from_sorted(
            rows.into_iter()
                .map(|(id, sort_name)| (from_row_id(id), sort_name)),
        ))
    }

    fn list_by_ids(&self, ids: &[GameId]) -> Result<Vec<GameListItem>> {
        use crate::schema::artwork;
        use crate::schema::games::dsl;

        let mut conn = self.conn()?;
        let mut by_id: HashMap<i64, GameListItem> = HashMap::with_capacity(ids.len());
        for chunk in ids.chunks(CHUNK_SIZE) {
            let row_ids: Vec<i64> = chunk.iter().map(|&id| to_row_id(id)).collect();
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
            .filter_map(|&id| by_id.remove(&to_row_id(id)))
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
        let mut conn = self.conn()?;
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

fn index_vocab(
    conn: &mut SqliteConnection,
    sort_names: impl IntoIterator<Item = String>,
) -> diesel::QueryResult<()> {
    use crate::schema::{games_spellfix, search_vocab};

    let mut words: BTreeSet<String> = BTreeSet::new();
    for name in sort_names {
        words.extend(title_words(&name).map(str::to_owned));
    }

    if words.is_empty() {
        return Ok(());
    }

    let words: Vec<String> = words.into_iter().collect();
    for chunk in words.chunks(VOCAB_CHUNK_SIZE) {
        let new_words: Vec<String> = diesel::insert_or_ignore_into(search_vocab::table)
            .values(
                chunk
                    .iter()
                    .map(|word| search_vocab::word.eq(word))
                    .collect::<Vec<_>>(),
            )
            .returning(search_vocab::word)
            .get_results(conn)?;

        if !new_words.is_empty() {
            diesel::insert_into(games_spellfix::table)
                .values(
                    new_words
                        .iter()
                        .map(|word| games_spellfix::word.eq(word))
                        .collect::<Vec<_>>(),
                )
                .execute(conn)?;
        }
    }

    Ok(())
}

/// Splits a normalized title like FTS5's `unicode61` tokenizer.
/// Drops one-character words, which aren't useful for spellfix.
fn title_words(sort_name: &str) -> impl Iterator<Item = &str> {
    sort_name
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.chars().nth(1).is_some())
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

struct Search {
    ids: Vec<GameId>,
    used_spellfix: bool,
}

impl Search {
    fn empty() -> Self {
        Self {
            ids: Vec::new(),
            used_spellfix: false,
        }
    }
    fn plain(ids: Vec<GameId>) -> Self {
        Self {
            ids,
            used_spellfix: false,
        }
    }
    fn spellfix(ids: Vec<GameId>) -> Self {
        Self {
            ids,
            used_spellfix: true,
        }
    }
}

const SEARCH_RESULT_LIMIT: i64 = 500;

const SPELLFIX_MIN_TOKEN_LEN: usize = 3;
/// Number of spellfix guesses. If too many remain after filtering, we leave
/// the word unchanged since it has no clear correction.
const SPELLFIX_CANDIDATES: usize = 10;
/// Limit corrections to the closest spellings to keep the query small.
const SPELLFIX_MAX_SUGGESTIONS: usize = 3;

fn search_games(conn: &mut SqliteConnection, normalized: &str) -> Result<Search> {
    let tokens: Vec<&str> = normalized.split_whitespace().collect();
    if tokens.is_empty() {
        return Ok(Search::empty());
    }

    let hits = ranked_search(conn, &fts_match_query(normalized), normalized)?;
    if !hits.is_empty() {
        return Ok(Search::plain(hits));
    }

    match spellfix_expand(conn, &tokens)? {
        Some(fuzzy) => Ok(Search::spellfix(ranked_search(conn, &fuzzy, normalized)?)),
        None => Ok(Search::empty()),
    }
}

fn fts_match_query(normalized: &str) -> String {
    normalized
        .split_whitespace()
        .map(prefix_term)
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(QueryableByName)]
struct IdRow {
    #[diesel(sql_type = BigInt)]
    id: i64,
}

#[derive(QueryableByName)]
struct WordRow {
    #[diesel(sql_type = Text)]
    word: String,
    #[diesel(sql_type = BigInt)]
    distance: i64,
}

fn ranked_search(
    conn: &mut SqliteConnection,
    fts_match: &str,
    normalized: &str,
) -> Result<Vec<GameId>> {
    let rows = sql_query(
        "SELECT g.id AS id \
         FROM games_fts \
         JOIN games g ON g.id = games_fts.rowid \
         WHERE games_fts MATCH ? \
         ORDER BY (g.sort_name = ?) DESC, \
                  (g.sort_name LIKE ? ESCAPE '\\') DESC, \
                  length(g.sort_name) ASC, \
                  g.sort_name ASC, \
                  g.id ASC \
         LIMIT ?",
    )
    .bind::<Text, _>(fts_match)
    .bind::<Text, _>(normalized)
    .bind::<Text, _>(format!("{}%", escape_like(normalized)))
    .bind::<BigInt, _>(SEARCH_RESULT_LIMIT)
    .load::<IdRow>(conn)?;

    Ok(rows.into_iter().map(|row| from_row_id(row.id)).collect())
}

/// Builds an FTS match string from spellfix suggestions
/// (e.g `("crimsn"* OR "crimson") AND ("empipe"* OR "empire")`).
/// Returns `None` if no token gets a correction.
fn spellfix_expand(conn: &mut SqliteConnection, tokens: &[&str]) -> Result<Option<String>> {
    let mut groups: Vec<String> = Vec::with_capacity(tokens.len() + 1);
    let mut corrected = false;

    // Multi-word queries need at least one literal prefix match to avoid noise.
    if tokens.len() > 1 {
        let anchor = tokens.iter().map(|t| prefix_term(t)).collect::<Vec<_>>();
        groups.push(format!("({})", anchor.join(" OR ")));
    }

    for token in tokens {
        let mut alternatives = vec![prefix_term(token)];

        if token.chars().count() >= SPELLFIX_MIN_TOKEN_LEN {
            // scope = 2 keeps suggestions close while allowing small typos.
            let mut words = sql_query(
                "SELECT word, distance FROM games_spellfix \
                 WHERE word MATCH ? AND top = ? AND scope = 2",
            )
            .bind::<Text, _>(*token)
            .bind::<BigInt, _>(SPELLFIX_CANDIDATES as i64)
            .load::<WordRow>(conn)?;

            // Skip ambiguous tokens or keep only the closest suggestions.
            words.retain(|w| w.distance <= max_edit_budget(token) && w.word != *token);
            if words.len() < SPELLFIX_CANDIDATES {
                words.sort_by_key(|w| w.distance);
                for WordRow { word, .. } in words.into_iter().take(SPELLFIX_MAX_SUGGESTIONS) {
                    corrected = true;
                    alternatives.push(format!("\"{}\"", escape_fts(&word)));
                }
            }
        }

        alternatives.sort();
        alternatives.dedup();
        groups.push(format!("({})", alternatives.join(" OR ")));
    }

    Ok(corrected.then(|| groups.join(" AND ")))
}

fn prefix_term(token: &str) -> String {
    format!("\"{}\"*", escape_fts(token))
}

/// Maximum edit distance for a correction. In spellfix around 100 units is
/// roughly one edit per three characters.
fn max_edit_budget(token: &str) -> i64 {
    (token.chars().count() as i64 * 40).max(120)
}

fn escape_fts(token: &str) -> String {
    token.replace('"', "\"\"")
}

fn escape_like(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

#[cfg(test)]
mod tests {
    use super::{fts_match_query, title_words};
    use crate::SqliteStorage;
    use domain::game::{GameRepository, GameSource, NewGame};
    use std::collections::HashMap;
    use tempfile::TempDir;

    #[track_caller]
    fn check_words(sort_name: &str, expected: &[&str]) {
        assert_eq!(title_words(sort_name).collect::<Vec<_>>(), expected);
    }

    #[test]
    fn title_words_splits_on_non_alphanumerics_and_drops_single_chars() {
        check_words("half-life 2", &["half", "life"]);
        check_words(".hack//g.u.", &["hack"]);
        check_words("assassin's creed", &["assassin", "creed"]);
        check_words("s.t.a.l.k.e.r.", &[]);
        check_words("", &[]);
    }

    #[test]
    fn blank_input_yields_empty_query() {
        assert_eq!(fts_match_query(""), "");
        assert_eq!(fts_match_query("   \t"), "");
    }

    #[test]
    fn tokens_are_quoted_and_prefixed() {
        assert_eq!(fts_match_query("half"), "\"half\"*");
        assert_eq!(fts_match_query("half life"), "\"half\"* \"life\"*");
    }

    #[test]
    fn operators_are_neutralized_as_literals() {
        assert_eq!(fts_match_query("a\"b"), "\"a\"\"b\"*");
        assert_eq!(fts_match_query("foo or bar"), "\"foo\"* \"or\"* \"bar\"*");
        assert_eq!(fts_match_query("summary:x"), "\"summary:x\"*");
    }

    fn library(names: &[&str]) -> (TempDir, SqliteStorage) {
        let dir = TempDir::new().unwrap();
        let storage = SqliteStorage::new(dir.path().join("library.sqlite")).unwrap();

        let games: Vec<NewGame> = names
            .iter()
            .map(|name| NewGame {
                name: (*name).to_string(),
                source: GameSource {
                    id: "test".to_string(),
                    lookup_id: (*name).to_string(),
                    external_ids: HashMap::new(),
                },
                cover_url: None,
                summary: None,
                description: None,
            })
            .collect();
        storage.insert_many(&games).unwrap();

        (dir, storage)
    }

    fn search(storage: &SqliteStorage, query: &str) -> Vec<String> {
        let index = storage.list_index(Some(query)).unwrap();
        storage
            .list_by_ids(&index.ids)
            .unwrap()
            .into_iter()
            .map(|item| item.name)
            .collect()
    }

    #[track_caller]
    fn check_search(names: &[&str], query: &str, expected: &[&str]) {
        let (_dir, storage) = library(names);
        let expected: Vec<String> = expected.iter().map(|s| (*s).to_string()).collect();
        assert_eq!(search(&storage, query), expected);
    }

    #[test]
    fn exact_title_ranks_above_longer_matches() {
        check_search(
            &["Half-Life 2", "Half-Life"],
            "half-life",
            &["Half-Life", "Half-Life 2"],
        );
    }

    #[test]
    fn starts_with_beats_mid_word_match() {
        check_search(
            &["Alpha Protocol", "Protocol"],
            "protocol",
            &["Protocol", "Alpha Protocol"],
        );
    }

    #[test]
    fn results_are_ordered_by_relevance_not_alphabetically() {
        // Alphabetically "Alpha Protocol" sorts first; by relevance the exact
        // match must win.
        check_search(
            &["Alpha Protocol", "Protocol"],
            "protocol",
            &["Protocol", "Alpha Protocol"],
        );
    }

    #[test]
    fn a_transposed_typo_still_finds_the_game() {
        check_search(&["Celeste", "Hades"], "celetse", &["Celeste"]);
    }

    #[test]
    fn a_dropped_letter_still_finds_the_game() {
        check_search(&["Half-Life", "Portal"], "half lfe", &["Half-Life"]);
    }

    #[test]
    fn a_clean_query_does_not_pull_in_typo_neighbours() {
        check_search(&["Doom", "Dune"], "doom", &["Doom"]);
    }

    #[test]
    fn one_real_word_still_anchors_a_multi_word_typo() {
        check_search(
            &["Rose Riddle", "Lila Sky Ark"],
            "rosa riddle",
            &["Rose Riddle"],
        );
    }

    #[test]
    fn a_multi_word_query_matching_nothing_literally_is_not_fuzzed() {
        // Both words mistyped and neither a prefix of a real title word: the
        // fuzzy pass must not stitch together unrelated titles.
        check_search(&["Celeste", "Hades Odyssey"], "celetse hodes", &[]);
    }

    #[test]
    fn a_typo_is_still_corrected_when_it_has_other_near_spellings() {
        // "mistery" is one edit from "mystery", "misery" and "mastery"; the
        // closest few are all folded in rather than the token being skipped.
        let results = {
            let (_dir, storage) = library(&["Mystery Box: Hidden Secrets", "Misery", "Mastery"]);
            search(&storage, "mistery")
        };
        assert!(results.contains(&"Mystery Box: Hidden Secrets".to_string()));
    }

    #[test]
    fn gibberish_matches_nothing() {
        check_search(&["Celeste", "Hades", "Portal"], "xqzptv", &[]);
    }

    #[test]
    fn searching_clears_the_section_index() {
        let (_dir, storage) = library(&["Celeste", "Hades"]);

        assert!(
            storage
                .list_index(Some("celeste"))
                .unwrap()
                .sections
                .is_empty()
        );
        assert!(!storage.list_index(None).unwrap().sections.is_empty());
    }
}
