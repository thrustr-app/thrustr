use crate::id::from_row_id;
use anyhow::Result;
use diesel::{
    ExpressionMethods, QueryableByName, RunQueryDsl, SqliteConnection, sql_query,
    sql_types::{BigInt, Text},
};
use domain::game::GameId;
use std::collections::BTreeSet;
use tracing::debug;

const MIN_TOKEN_LEN: usize = 3;
const SPELLFIX_CANDIDATES: i64 = 5;
const MAX_SUGGESTIONS: usize = 2;
const SPELLFIX_SCOPE: i64 = 2;
const MAX_EDIT_DISTANCE: i64 = 200;
const SEARCH_RESULT_LIMIT: i64 = 200;
const VOCAB_CHUNK_SIZE: usize = 500;

/// Games matching `normalized`, best first. Empty when nothing matches.
pub(super) fn search(conn: &mut SqliteConnection, normalized: &str) -> Result<Vec<GameId>> {
    let tokens: Vec<&str> = normalized.split_whitespace().collect();
    if tokens.is_empty() {
        return Ok(Vec::new());
    }

    let mut hits = ranked_search(conn, &fts_match_query(&tokens), normalized)?;
    let mut used_spellfix = false;

    if hits.is_empty()
        && let Some(fuzzy) = spellfix_expand(conn, &tokens)?
    {
        used_spellfix = true;
        hits = ranked_search(conn, &fuzzy, normalized)?;
    }

    debug!(used_spellfix, hits = hits.len(), "library search");
    Ok(hits)
}

/// Orders results by exact match, then prefix match, then shortest.
fn ranked_search(
    conn: &mut SqliteConnection,
    fts_match: &str,
    normalized: &str,
) -> Result<Vec<GameId>> {
    #[derive(QueryableByName)]
    struct IdRow {
        #[diesel(sql_type = BigInt)]
        id: i64,
    }

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

fn fts_match_query(tokens: &[&str]) -> String {
    prefix_terms(tokens).join(" ")
}

/// Builds a fuzzy query with spelling suggestions, e.g.
/// `("crimsn"* OR "crimson") AND ("empipe"* OR "empire")`.
/// Returns `None` if nothing gets corrected.
fn spellfix_expand(conn: &mut SqliteConnection, tokens: &[&str]) -> Result<Option<String>> {
    let mut groups: Vec<String> = Vec::with_capacity(tokens.len() + 1);
    let mut corrected = false;

    if tokens.len() > 1 {
        groups.push(literal_anchor(tokens));
    }

    for token in tokens {
        let mut alternatives = vec![prefix_term(token)];

        for word in suggestions(conn, token)? {
            corrected = true;
            alternatives.push(format!("\"{}\"", escape_fts(&word)));
        }

        groups.push(format!("({})", alternatives.join(" OR ")));
    }

    Ok(corrected.then(|| groups.join(" AND ")))
}

fn literal_anchor(tokens: &[&str]) -> String {
    format!("({})", prefix_terms(tokens).join(" OR "))
}

fn suggestions(conn: &mut SqliteConnection, token: &str) -> Result<Vec<String>> {
    #[derive(QueryableByName)]
    struct WordRow {
        #[diesel(sql_type = Text)]
        word: String,
    }

    if token.chars().count() < MIN_TOKEN_LEN {
        return Ok(Vec::new());
    }

    let rows = sql_query(
        "SELECT word \
         FROM games_spellfix \
         WHERE word MATCH ? AND top = ? AND scope = ? AND distance <= ?",
    )
    .bind::<Text, _>(token)
    .bind::<BigInt, _>(SPELLFIX_CANDIDATES)
    .bind::<BigInt, _>(SPELLFIX_SCOPE)
    .bind::<BigInt, _>(MAX_EDIT_DISTANCE)
    .load::<WordRow>(conn)?;

    Ok(rows
        .into_iter()
        .map(|row| row.word)
        .filter(|word| word != token)
        .take(MAX_SUGGESTIONS)
        .collect())
}

fn prefix_terms(tokens: &[&str]) -> Vec<String> {
    tokens.iter().map(|token| prefix_term(token)).collect()
}

fn prefix_term(token: &str) -> String {
    format!("\"{}\"*", escape_fts(token))
}

fn escape_fts(token: &str) -> String {
    token.replace('"', "\"\"")
}

fn escape_like(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// Adds words to the spellfix vocabulary.
///
/// Uses `search_vocab` to track what's been added since
/// spellfix1 doesn't support unique constraints.
pub(super) fn index_vocab(
    conn: &mut SqliteConnection,
    words: BTreeSet<String>,
) -> diesel::QueryResult<()> {
    use crate::schema::{games_spellfix, search_vocab};

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

pub(super) fn vocab_words<'a>(sort_names: impl IntoIterator<Item = &'a str>) -> BTreeSet<String> {
    sort_names
        .into_iter()
        .flat_map(title_words)
        .map(str::to_owned)
        .collect()
}

fn title_words(sort_name: &str) -> impl Iterator<Item = &str> {
    sort_name
        .split_whitespace()
        .filter(|word| word.chars().count() >= MIN_TOKEN_LEN)
}
