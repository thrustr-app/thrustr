// SPDX-License-Identifier: GPL-3.0-or-later
//
// Parts of this module are adapted from the gpui text-input example,
// Copyright (C) Zed Industries, Inc., licensed under Apache-2.0:
// https://github.com/zed-industries/zed/blob/main/crates/gpui/examples/input.rs
//
// Modified and redistributed as part of Thrustr under GPL-3.0-or-later.

use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

/// Character type for word boundary detection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CharType {
    Whitespace,
    Word,
    Punctuation,
}

pub struct TextOps;

impl TextOps {
    /// Get the previous grapheme boundary from the given offset
    pub fn previous_boundary(text: &str, offset: usize) -> usize {
        if offset == 0 {
            return 0;
        }
        text.grapheme_indices(true)
            .take_while(|(i, _)| *i < offset)
            .map(|(i, _)| i)
            .last()
            .unwrap_or(0)
    }

    /// Get the next grapheme boundary from the given offset
    pub fn next_boundary(text: &str, offset: usize) -> usize {
        if offset >= text.len() {
            return text.len();
        }
        text.grapheme_indices(true)
            .find(|(i, _)| *i > offset)
            .map(|(i, _)| i)
            .unwrap_or(text.len())
    }

    /// Get the previous word boundary from the given offset
    pub fn previous_word_boundary(text: &str, offset: usize) -> usize {
        if offset == 0 {
            return 0;
        }

        let mut iter = text.char_indices().rev().peekable();
        let mut found_non_whitespace = false;
        let mut last_char_type = None;
        let mut prev_ch = None;

        while let Some((i, ch)) = iter.next() {
            if i >= offset {
                prev_ch = Some(ch);
                continue;
            }

            let next_ch = iter.peek().map(|&(_, c)| c);
            let char_type = Self::char_type(ch, next_ch, prev_ch);

            if !found_non_whitespace && char_type != CharType::Whitespace {
                found_non_whitespace = true;
                last_char_type = Some(char_type);
                prev_ch = Some(ch);
                continue;
            }

            if found_non_whitespace
                && let Some(last_type) = last_char_type
                && (char_type != last_type || char_type == CharType::Whitespace)
            {
                return Self::next_boundary(text, i);
            }

            last_char_type = Some(char_type);
            prev_ch = Some(ch);
        }

        0
    }

    /// Get the next word boundary from the given offset
    pub fn next_word_boundary(text: &str, offset: usize) -> usize {
        if offset >= text.len() {
            return text.len();
        }

        let mut iter = text.char_indices().peekable();
        let mut found_non_whitespace = false;
        let mut last_char_type = None;
        let mut prev_ch = None;

        while let Some((i, ch)) = iter.next() {
            if i < offset {
                prev_ch = Some(ch);
                continue;
            }

            let next_ch = iter.peek().map(|&(_, c)| c);
            let char_type = Self::char_type(ch, next_ch, prev_ch);

            if !found_non_whitespace && char_type != CharType::Whitespace {
                found_non_whitespace = true;
                last_char_type = Some(char_type);
                prev_ch = Some(ch);
                continue;
            }

            if found_non_whitespace
                && let Some(last_type) = last_char_type
                && (char_type != last_type || char_type == CharType::Whitespace)
            {
                return i;
            }

            last_char_type = Some(char_type);
            prev_ch = Some(ch);
        }

        text.len()
    }

    /// Determine the character type for word boundary detection
    fn char_type(ch: char, next: Option<char>, prev: Option<char>) -> CharType {
        if ch.is_whitespace() {
            CharType::Whitespace
        } else if ch.is_alphanumeric()
            || ch == '_'
            || (ch == '.'
                && prev.is_some_and(|c| c.is_ascii_digit())
                && next.is_some_and(|c| c.is_ascii_digit()))
        {
            CharType::Word
        } else {
            CharType::Punctuation
        }
    }

    /// Snap a byte offset to the closest grapheme boundary at or before it
    ///
    /// Offsets past the end of the text are clamped to `text.len()`.
    pub fn snap_to_grapheme_boundary(text: &str, offset: usize) -> usize {
        if offset >= text.len() {
            return text.len();
        }
        text.grapheme_indices(true)
            .take_while(|(i, _)| *i <= offset)
            .last()
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    /// Convert a grapheme offset to a byte offset
    pub fn grapheme_offset_to_byte_offset(text: &str, grapheme_offset: usize) -> usize {
        text.grapheme_indices(true)
            .nth(grapheme_offset)
            .map(|(i, _)| i)
            .unwrap_or(text.len())
    }

    /// Convert offset to UTF-16 code units
    pub fn offset_to_utf16(text: &str, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut byte_offset = 0;

        for ch in text.chars() {
            if byte_offset >= offset {
                break;
            }
            utf16_offset += ch.len_utf16();
            byte_offset += ch.len_utf8();
        }

        utf16_offset
    }

    /// Convert UTF-16 offset to byte offset
    pub fn offset_from_utf16(text: &str, utf16_offset: usize) -> usize {
        let mut current_utf16_offset = 0;
        let mut byte_offset = 0;

        for ch in text.chars() {
            if current_utf16_offset >= utf16_offset {
                break;
            }
            current_utf16_offset += ch.len_utf16();
            byte_offset += ch.len_utf8();
        }

        byte_offset
    }

    /// Convert a byte range to UTF-16 range
    pub fn range_to_utf16(text: &str, range: &Range<usize>) -> Range<usize> {
        Self::offset_to_utf16(text, range.start)..Self::offset_to_utf16(text, range.end)
    }

    /// Convert a UTF-16 range to byte range
    pub fn range_from_utf16(text: &str, range: &Range<usize>) -> Range<usize> {
        Self::offset_from_utf16(text, range.start)..Self::offset_from_utf16(text, range.end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `expected[i]` is the snapped result for byte offset `i` including
    /// one past the end of `text`.
    #[track_caller]
    fn check_snap(text: &str, expected: &[usize]) {
        let actual: Vec<usize> = (0..=text.len() + 1)
            .map(|offset| TextOps::snap_to_grapheme_boundary(text, offset))
            .collect();
        assert_eq!(actual, expected, "snapping every offset of {text:?}");
    }

    #[test]
    fn snap_clamps_offsets_past_end() {
        check_snap("abc", &[0, 1, 2, 3, 3]);
        check_snap("", &[0, 0]);
    }

    #[test]
    fn snap_moves_offsets_inside_multibyte_char_to_start() {
        check_snap("日本", &[0, 0, 0, 3, 3, 3, 6, 6]);
    }

    #[test]
    fn snap_keeps_combining_marks_with_base() {
        check_snap("e\u{301}x", &[0, 0, 0, 3, 4, 4]);
    }

    /// `fixture` is the text annotated with `|` at the cursor and `[` and `]`
    /// at the expected previous and next word boundaries.
    #[track_caller]
    fn check_word_boundaries(fixture: &str) {
        let mut text = String::new();
        let mut cursor = None;
        for ch in fixture.chars() {
            match ch {
                '|' => cursor = Some(text.len()),
                '[' | ']' => {}
                _ => text.push(ch),
            }
        }
        let cursor = cursor.expect("fixture should contain a `|` cursor");

        let prev = TextOps::previous_word_boundary(&text, cursor);
        let next = TextOps::next_word_boundary(&text, cursor);

        let mut marks = [(cursor, '|'), (prev, '['), (next, ']')];
        marks.sort_by_key(|&(offset, _)| std::cmp::Reverse(offset));
        let mut actual = text;
        for (offset, mark) in marks {
            actual.insert(offset, mark);
        }

        assert_eq!(actual, fixture);
    }

    #[test]
    fn word_boundaries_split_on_whitespace() {
        check_word_boundaries("[|hello] world");
        check_word_boundaries("[hello| world]");
        check_word_boundaries("[hello |world]");
        check_word_boundaries("[|hello]  world");
        check_word_boundaries("[hello|  world]");
        check_word_boundaries("[hello | world]");
        check_word_boundaries("[|  hello] world  ");
        check_word_boundaries("  [hell|o] world  ");
        check_word_boundaries("  [hello| world]  ");
    }

    #[test]
    fn whitespace_only_text_skips_to_ends() {
        check_word_boundaries("[|   ]");
    }

    #[test]
    fn punctuation_runs_form_separate_words() {
        check_word_boundaries("[|hello], world!");
        check_word_boundaries("[hello|,] world!");
        check_word_boundaries("hello[,| world]!");
        check_word_boundaries("hello[.|..] world!");
        check_word_boundaries("[|hello]@world.com");
        check_word_boundaries("[hello|@]world.com");
        check_word_boundaries("hello[@|world].com");
    }

    #[test]
    fn underscores_join_words_hyphens_split() {
        check_word_boundaries("[|hello]-world_test");
        check_word_boundaries("[hello|-]world_test");
        check_word_boundaries("hello[-|world_test]");
    }

    #[test]
    fn decimal_numbers_stay_together() {
        check_word_boundaries("[|123] 456");
        check_word_boundaries("[12|3] 456");
        check_word_boundaries("[123| 456]");
        check_word_boundaries("[|123.456]");
        check_word_boundaries("[12|3.456]");
        check_word_boundaries("[123|.456]");
        check_word_boundaries("[1.23e|10]");
    }

    #[test]
    fn emojis_are_separate_words() {
        check_word_boundaries("[|hello] 👋 world");
        check_word_boundaries("[hello| 👋] world");
        check_word_boundaries("[hello |👋] world");
        check_word_boundaries("[|👋] hello world");
        check_word_boundaries("[👋| hello] world");
        check_word_boundaries("👋 [he|llo] world");
    }

    #[test]
    fn identifiers_split_on_punctuation() {
        check_word_boundaries("[|file_name_v2]-final.txt");
        check_word_boundaries("[file_name_v2|-]final.txt");
        check_word_boundaries("file_name_v2[-|final].txt");
        check_word_boundaries("file_name_v2-[final|.]txt");
        check_word_boundaries("file_name_v2-final[.|txt]");
        check_word_boundaries("[|the] quick-brown_fox42 jumps!");
        check_word_boundaries("[the| quick]-brown_fox42 jumps!");
        check_word_boundaries("the [quick|-]brown_fox42 jumps!");
        check_word_boundaries("the quick[-|brown_fox42] jumps!");
        check_word_boundaries("the quick-[brown_fox42| jumps]!");
        check_word_boundaries("the quick-brown_fox42 [jumps|!]");
    }
}
