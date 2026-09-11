use crate::{
    artwork::{Artwork, ArtworkKind},
    id::Id,
    platform::Platform,
    section_index::{SectionIndex, name_bucket},
};
use anyhow::Result;
use std::collections::HashMap;
use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

pub type GameId = Id<Game>;

pub trait GameExt {
    fn name(&self) -> &str;
    fn sort_name(&self) -> String {
        normalize(self.name())
    }
}

pub fn normalize(text: &str) -> String {
    word_groups(text).concat().join(" ")
}

/// Splits text into word groups: `Marvel's Spider-Man` -> `[["marvels"], ["spider", "man"]]`.
/// Whitespace starts a new group while other punctuation ends the current word.
pub fn word_groups(text: &str) -> Vec<Vec<String>> {
    fold(text)
        .split_whitespace()
        .filter_map(|chunk| {
            let words: Vec<String> = chunk
                .split(|c: char| !c.is_alphanumeric())
                .filter(|word| !word.is_empty())
                .map(String::from)
                .collect();
            (!words.is_empty()).then_some(words)
        })
        .collect()
}

/// Strips diacritics and apostrophes and lowercases.
fn fold(text: &str) -> String {
    text.nfd()
        .filter(|c| !is_combining_mark(*c) && !is_apostrophe(*c))
        .flat_map(char::to_lowercase)
        .collect()
}

fn is_apostrophe(c: char) -> bool {
    matches!(c, '\'' | '\u{2019}')
}

#[derive(Debug)]
pub struct Game {
    pub id: GameId,
    pub name: String,
    pub source: GameSource,
    /// The URL of the original cover art for the game, as provided by the storefront.
    pub cover_url: Option<String>,
    pub cover: Option<Artwork>,
    pub summary: Option<String>,
    pub description: Option<String>,
}

impl GameExt for Game {
    fn name(&self) -> &str {
        &self.name
    }
}

#[derive(Debug)]
pub struct GameSource {
    /// The identifier for the game source (e.g. "steam", "gog").
    pub id: String,
    /// The unique identifier for the game in the source. This is usually a specific
    /// identifier (e.g. Steam App ID) or a combination of multiple identifiers.
    pub lookup_id: String,
    /// Arbitrary external identifiers to be consumed by components.
    pub external_ids: HashMap<String, String>,
}

#[derive(Debug)]
pub struct GameVersion {
    pub id: String,
    pub pretty_name: Option<String>,
    pub platform: Platform,
}

#[derive(Debug)]
pub struct NewGame {
    pub name: String,
    pub source: GameSource,
    pub cover_url: Option<String>,
    pub summary: Option<String>,
    pub description: Option<String>,
}

impl GameExt for NewGame {
    fn name(&self) -> &str {
        &self.name
    }
}

#[derive(Debug)]
pub struct GameListItem {
    pub id: GameId,
    pub name: String,
    pub source_id: String,
    pub cover_url: Option<String>,
    pub cover: Option<Artwork>,
}

/// Ordered game IDs and their section boundaries.
#[derive(Debug, Default)]
pub struct GameIndex {
    pub ids: Vec<GameId>,
    pub sections: SectionIndex,
}

impl GameIndex {
    /// Builds an index without sections.
    pub fn from_ids(ids: impl IntoIterator<Item = GameId>) -> Self {
        Self {
            ids: ids.into_iter().collect(),
            sections: SectionIndex::default(),
        }
    }

    /// Builds an index from `(id, sort_name)` pairs already ordered by sort
    /// name, then id.
    pub fn from_sorted(items: impl IntoIterator<Item = (GameId, String)>) -> Self {
        let items = items.into_iter();
        let mut ids = Vec::with_capacity(items.size_hint().0);
        let sections = SectionIndex::from_buckets(items.map(|(id, sort_name)| {
            ids.push(id);
            name_bucket(&sort_name)
        }));

        Self { ids, sections }
    }
}

pub trait GameRepository: Send + Sync {
    /// `None` if the game already exists.
    fn insert(&self, game: &NewGame) -> Result<Option<Game>>;

    fn insert_many(&self, games: &[NewGame]) -> Result<usize>;

    fn get(&self, id: GameId) -> Result<Option<Game>>;

    fn list_index(&self, query: Option<&str>) -> Result<GameIndex>;

    fn list_by_ids(&self, ids: &[GameId]) -> Result<Vec<GameListItem>>;

    /// Games with a source URL for `kind` but no stored artwork yet, as
    /// `(id, source url)` pairs. Ordered by id, starting after `after`, so the
    /// last id returned is the cursor for the next page.
    fn list_missing_artwork(
        &self,
        kind: ArtworkKind,
        after: GameId,
        limit: usize,
    ) -> Result<Vec<(GameId, String)>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Named(String);

    impl GameExt for Named {
        fn name(&self) -> &str {
            &self.0
        }
    }

    #[track_caller]
    fn check_sort_name(name: &str, expected: &str) {
        assert_eq!(Named(name.to_string()).sort_name(), expected);
        assert_eq!(normalize(name), expected);
    }

    #[test]
    fn sort_name_normalizes_game_name() {
        check_sort_name("", "");
        check_sort_name("Zelda", "zelda");
        check_sort_name("ZELDA", "zelda");
        check_sort_name("7 Days to Die", "7 days to die");
        check_sort_name(".hack//G.U.", "hack g u");
        check_sort_name("S.T.A.L.K.E.R.", "s t a l k e r");
        check_sort_name("Marvel's Spider-Man", "marvels spider man");
        check_sort_name("Marvel’s Spider-Man", "marvels spider man");
        check_sort_name("Café", "cafe");
        check_sort_name(" naïve ", "naive");
        check_sort_name("São Paulo", "sao paulo");
        check_sort_name("Å", "a");
        check_sort_name("Straße", "straße");
    }

    #[track_caller]
    fn check_groups(name: &str, expected: &[&[&str]]) {
        let got = word_groups(name);
        let got: Vec<Vec<&str>> = got
            .iter()
            .map(|group| group.iter().map(String::as_str).collect())
            .collect();
        assert_eq!(got, expected);
    }

    #[test]
    fn splits_into_groups() {
        check_groups(
            "The Legend of Zelda",
            &[&["the"], &["legend"], &["of"], &["zelda"]],
        );
        check_groups("Spider-Man", &[&["spider", "man"]]);
        check_groups("Marvel's Spider-Man", &[&["marvels"], &["spider", "man"]]);
        check_groups(
            "S.T.A.L.K.E.R.: Shadow of Chernobyl",
            &[
                &["s", "t", "a", "l", "k", "e", "r"],
                &["shadow"],
                &["of"],
                &["chernobyl"],
            ],
        );
        check_groups("", &[]);
        check_groups("  ", &[]);
        check_groups("--", &[]);
        check_groups("foo -- bar", &[&["foo"], &["bar"]]);
        check_groups("-foo-", &[&["foo"]]);
        check_groups("Half-Life 2", &[&["half", "life"], &["2"]]);
    }
}
