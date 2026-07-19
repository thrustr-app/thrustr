use crate::{
    artwork::{Artwork, ArtworkKind},
    id::Id,
    platform::Platform,
};
use anyhow::Result;
use std::collections::HashMap;

pub type GameId = Id<Game>;

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

#[derive(Debug)]
pub struct GameSource {
    /// The identifier for the game source (e.g. "steam", "gog").
    pub id: String,
    /// The unique identifier for the game in the source. This usually is a specific
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

#[derive(Debug)]
pub struct GameListItem {
    pub id: GameId,
    pub name: String,
    pub source_id: String,
    pub cover_url: Option<String>,
    pub cover: Option<Artwork>,
}

/// The ordered id list backing the library, plus the section boundaries within
/// it.
///
/// Both are produced together because the sections must describe exactly the
/// order `ids` is in.
#[derive(Debug, Default)]
pub struct GameIndex {
    pub ids: Vec<GameId>,
    pub sections: SectionIndex,
}

/// A contiguous run of games sharing a sort label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub label: String,
    pub start: usize,
}

/// Section boundaries over an ordered game list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SectionIndex(Vec<Section>);

impl SectionIndex {
    pub fn sections(&self) -> &[Section] {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Label of the section containing `index`.
    pub fn label_for(&self, index: usize) -> Option<&str> {
        let next = self.0.partition_point(|section| section.start <= index);
        Some(self.0.get(next.checked_sub(1)?)?.label.as_str())
    }

    /// First index carrying `label`.
    pub fn start_of(&self, label: &str) -> Option<usize> {
        self.0
            .iter()
            .find(|section| section.label == label)
            .map(|section| section.start)
    }
}

impl FromIterator<char> for SectionIndex {
    fn from_iter<I: IntoIterator<Item = char>>(buckets: I) -> Self {
        let mut sections = Vec::new();
        let mut current = None;
        for (start, bucket) in buckets.into_iter().enumerate() {
            if current == Some(bucket) {
                continue;
            }
            current = Some(bucket);
            sections.push(Section {
                label: bucket.to_string(),
                start,
            });
        }
        Self(sections)
    }
}

/// Groups a game name under `#` or `A`-`Z` for the library index.
pub fn name_bucket(name: &str) -> char {
    name.chars()
        .find(|c| !c.is_whitespace())
        .map(|c| c.to_ascii_uppercase())
        .filter(char::is_ascii_alphabetic)
        .unwrap_or('#')
}

pub trait GameRepository: Send + Sync {
    fn insert(&self, game: &NewGame) -> Result<Option<Game>>;

    fn insert_many(&self, games: &[NewGame]) -> Result<usize>;

    fn get(&self, id: GameId) -> Result<Option<Game>>;

    fn list_index(&self) -> Result<GameIndex>;

    fn list_by_ids(&self, ids: &[GameId]) -> Result<Vec<GameListItem>>;

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

    fn index(names: &[&str]) -> SectionIndex {
        names.iter().map(|name| name_bucket(name)).collect()
    }

    #[test]
    fn buckets_names() {
        assert_eq!(name_bucket("Portal"), 'P');
        assert_eq!(name_bucket("portal"), 'P');
        assert_eq!(name_bucket("  Portal"), 'P');
        assert_eq!(name_bucket("7 Days to Die"), '#');
        assert_eq!(name_bucket("!Sokoban"), '#');
        assert_eq!(name_bucket(""), '#');
    }

    #[test]
    fn labels_every_position_in_a_run() {
        let index = index(&["Alpha", "Amber", "Beta", "Zeta"]);

        assert_eq!(index.label_for(0), Some("A"));
        assert_eq!(index.label_for(1), Some("A"));
        assert_eq!(index.label_for(2), Some("B"));
        assert_eq!(index.label_for(3), Some("Z"));
    }

    #[test]
    fn labels_positions_past_the_last_section() {
        let index = index(&["Alpha"]);

        assert_eq!(index.label_for(99), Some("A"));
        assert_eq!(SectionIndex::default().label_for(0), None);
    }

    #[test]
    fn finds_the_first_run_of_a_label() {
        let index = index(&["Alpha", "Zeta", "alpha"]);

        assert_eq!(index.sections().len(), 3);
        assert_eq!(index.start_of("A"), Some(0));
        assert_eq!(index.label_for(2), Some("A"));
        assert_eq!(index.start_of("Q"), None);
    }
}
