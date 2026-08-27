/// Section boundaries for an ordered list of items.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SectionIndex(Vec<Section>);

/// A group of consecutive items with the same label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub label: String,
    pub start: usize,
}

impl SectionIndex {
    pub fn from_buckets<B>(buckets: impl IntoIterator<Item = B>) -> Self
    where
        B: Clone + PartialEq + Into<String>,
    {
        let mut sections: Vec<Section> = Vec::new();
        let mut last: Option<B> = None;
        for (start, bucket) in buckets.into_iter().enumerate() {
            if last.as_ref() == Some(&bucket) {
                continue;
            }
            last = Some(bucket.clone());
            sections.push(Section {
                label: bucket.into(),
                start,
            });
        }
        Self(sections)
    }

    pub fn sections(&self) -> &[Section] {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the label for the given index.
    pub fn label_for(&self, index: usize) -> Option<&str> {
        // Find the last section whose start <= index.
        let next = self.0.partition_point(|section| section.start <= index);
        Some(self.0.get(next.checked_sub(1)?)?.label.as_str())
    }

    /// Returns the first index with the given label.
    pub fn start_of(&self, label: &str) -> Option<usize> {
        self.0
            .iter()
            .find(|section| section.label == label)
            .map(|section| section.start)
    }
}

/// Puts a sort name into `#` or `A`-`Z`.
pub fn name_bucket(sort_name: &str) -> char {
    sort_name
        .chars()
        .next()
        .filter(char::is_ascii_alphabetic)
        .map(|c| c.to_ascii_uppercase())
        .unwrap_or('#')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[track_caller]
    fn check_name_bucket(sort_name: &str, expected: char) {
        assert_eq!(name_bucket(sort_name), expected);
    }

    #[test]
    fn name_bucket_classifies_name() {
        check_name_bucket("zelda", 'Z');
        check_name_bucket("Zelda", 'Z');
        check_name_bucket("", '#');
        check_name_bucket("7 days to die", '#');
        check_name_bucket(".hack//g.u.", '#');
        // name_bucket does not normalize.
        check_name_bucket("émile", '#');
    }

    #[track_caller]
    fn check_index(buckets: &str, queries: &[(usize, Option<char>)]) {
        let index = SectionIndex::from_buckets(buckets.chars());
        for &(i, expected) in queries {
            let actual = index.label_for(i).map(|s| s.chars().next().unwrap());
            assert_eq!(actual, expected, "label_for({i}) over buckets {buckets:?}");
        }
    }

    #[test]
    fn empty_section_index_is_none() {
        let index = SectionIndex::from_buckets(std::iter::empty::<char>());
        assert!(index.is_empty());
        assert_eq!(index.label_for(0), None);
        assert_eq!(index.start_of("a"), None);
    }

    #[test]
    fn section_index_is_obtained_from_buckets() {
        check_index("aaaa", &[(0, Some('a')), (3, Some('a'))]);
        check_index(
            "aaabbc",
            &[
                (0, Some('a')),
                (2, Some('a')),
                (3, Some('b')),
                (4, Some('b')),
                (5, Some('c')),
            ],
        );
    }

    #[test]
    fn section_index_computes_sections_and_start_of() {
        let index = SectionIndex::from_buckets("aaabbc".chars());

        let labels: Vec<&str> = index.sections().iter().map(|s| s.label.as_str()).collect();
        assert_eq!(labels, vec!["a", "b", "c"]);

        assert_eq!(index.start_of("a"), Some(0));
        assert_eq!(index.start_of("b"), Some(3));
        assert_eq!(index.start_of("c"), Some(5));
        assert_eq!(index.start_of("z"), None);
    }
}
