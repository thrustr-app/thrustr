use domain::game::{GameIndex, GameRepository, GameSource, NewGame};
use sqlite::SqliteStorage;
use std::collections::HashMap;
use tempfile::TempDir;

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
fn check_search_exact(names: &[&str], query: &str, expected: &[&str]) {
    let (_dir, storage) = library(names);
    let expected: Vec<String> = expected.iter().map(|s| (*s).to_string()).collect();
    assert_eq!(search(&storage, query), expected);
}

#[track_caller]
fn check_search_contained(names: &[&str], query: &str, expected: &str) {
    let (_dir, storage) = library(names);
    let results = search(&storage, query);
    assert!(
        results.iter().any(|name| name == expected),
        "expected {expected:?} among {results:?}"
    );
}

#[track_caller]
fn index_of(names: &[&str], query: Option<&str>) -> GameIndex {
    let (_dir, storage) = library(names);
    storage.list_index(query).unwrap()
}

#[test]
fn exact_match_ranks_first() {
    check_search_exact(
        &["Half-Life 2", "Half-Life"],
        "half-life",
        &["Half-Life", "Half-Life 2"],
    );
}

#[test]
fn prefix_beats_midword_match() {
    check_search_exact(
        &["Alpha Protocol", "Protocol"],
        "protocol",
        &["Protocol", "Alpha Protocol"],
    );
}

#[test]
fn partial_word_finds_match() {
    check_search_exact(&["Celeste", "Hades"], "cele", &["Celeste"]);
    check_search_exact(&["Celeste", "Hades"], "hade", &["Hades"]);
}

#[test]
fn hyphenated_title_matches() {
    check_search_exact(&["Spider-Man", "Hades"], "spiderman", &["Spider-Man"]);
    check_search_exact(&["Spider-Man", "Hades"], "spider man", &["Spider-Man"]);
    check_search_exact(&["Spider-Man", "Hades"], "spider-man", &["Spider-Man"]);
}

#[test]
fn acronym_matches_collapsed_form() {
    check_search_exact(
        &["S.T.A.L.K.E.R.: Shadow of Chernobyl", "Portal"],
        "stalker",
        &["S.T.A.L.K.E.R.: Shadow of Chernobyl"],
    );
}

#[test]
fn compound_finds_midword_match() {
    check_search_exact(
        &["Marvel's Spider-Man", "Celeste"],
        "spiderman",
        &["Marvel's Spider-Man"],
    );
}

#[test]
fn possessive_matches_without_apostrophe() {
    check_search_exact(
        &["Baldur's Gate 3", "Hades"],
        "baldurs gate",
        &["Baldur's Gate 3"],
    );
}

#[test]
fn long_joined_title_is_not_concatenated() {
    let name = "aaaaaaaaaa-bbbbbbbbbb-cccccccccc-dddddddddd-eeeeeeeeee-ffffffffff-gggggggggg";
    check_search_exact(&[name], "ffffffffff", &[name]);
    check_search_exact(
        &[name],
        "aaaaaaaaaabbbbbbbbbbccccccccccddddddddddeeeeeeeeeeffffffffffgggggggggg",
        &[],
    );
}

#[test]
fn punctuation_is_stripped_from_query() {
    check_search_exact(&["Half-Life"], "\"half-life\"*", &["Half-Life"]);
    check_search_exact(
        &["100% Orange Juice", "Celeste"],
        "100% orange",
        &["100% Orange Juice"],
    );
}

#[test]
fn boolean_keywords_are_trated_as_words() {
    check_search_exact(&["Doom", "Quake"], "doom or quake", &[]);
    check_search_exact(
        &["Now or Never", "Celeste"],
        "now or never",
        &["Now or Never"],
    );
}

#[test]
fn transposed_typo_finds_match() {
    check_search_exact(&["Celeste", "Hades"], "celetse", &["Celeste"]);
}

#[test]
fn dropped_letter_finds_match() {
    check_search_exact(&["Half-Life", "Portal"], "half lfe", &["Half-Life"]);
}

#[test]
fn substituted_letter_finds_short_title() {
    check_search_exact(&["Rust", "Portal"], "rest", &["Rust"]);
}

#[test]
fn clean_query_excludes_fuzzy_neighbours() {
    check_search_exact(&["Doom", "Dune"], "doom", &["Doom"]);
}

#[test]
fn all_wrong_words_rejects_fuzzy() {
    check_search_exact(&["Celeste", "Hades Odyssey"], "celetse hodes", &[]);
}

#[test]
fn typo_finds_near_spelling() {
    check_search_contained(
        &["Mystery Box: Hidden Secrets", "Misery", "Mastery"],
        "mistery",
        "Mystery Box: Hidden Secrets",
    );
}

#[test]
fn hyphenated_typo_uses_components() {
    check_search_exact(&["Spider-Man", "Hades"], "spidermn", &["Spider-Man"]);
}

#[test]
fn gibberish_matches_nothing() {
    check_search_exact(&["Celeste", "Hades", "Portal"], "xqzptv", &[]);
}

#[test]
fn punctuation_only_returns_nothing() {
    check_search_exact(&["Celeste", "Hades"], "---", &[]);
}

#[test]
fn blank_query_browses_the_whole_library() {
    check_search_exact(&["Celeste", "Hades"], "", &["Celeste", "Hades"]);
    check_search_exact(&["Celeste", "Hades"], "   ", &["Celeste", "Hades"]);
}

#[test]
fn search_creates_flat_index() {
    assert!(
        index_of(&["Celeste", "Hades"], Some("celeste"))
            .sections
            .is_empty()
    );
    assert!(!index_of(&["Celeste", "Hades"], None).sections.is_empty());
}

#[test]
fn non_latin_groups_in_hash_section() {
    let index = index_of(
        &["Zelda", "7 Days to Die", "Ørsted", "Тетрис", "Celeste"],
        None,
    );

    let labels: Vec<&str> = index
        .sections
        .sections()
        .iter()
        .map(|section| section.label.as_str())
        .collect();

    assert_eq!(labels, vec!["#", "C", "Z"]);
    assert_eq!(index.sections.start_of("#"), Some(0));
}
