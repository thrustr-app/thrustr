use gpui::SharedString;
use std::collections::VecDeque;
use std::ops::Range;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    Insert {
        range: Range<usize>,
        text: SharedString,
    },
    Delete {
        range: Range<usize>,
        text: SharedString,
    },
    Replace {
        range: Range<usize>,
        old_text: SharedString,
        new_text: SharedString,
        marked: bool,
    },
}

impl Change {
    fn inverse(self) -> Change {
        match self {
            Change::Insert { range, text } => Change::Delete {
                range: range.start..range.start + text.len(),
                text: SharedString::new(""),
            },
            Change::Delete { range, text } => Change::Insert {
                range: range.start..range.start,
                text,
            },
            Change::Replace {
                range,
                old_text,
                new_text,
                marked,
            } => Change::Replace {
                range: range.start..range.start + new_text.len(),
                old_text: new_text,
                new_text: old_text,
                marked,
            },
        }
    }

    pub fn text(&self) -> SharedString {
        match self {
            Change::Insert { text, .. } => text.clone(),
            Change::Delete { .. } => SharedString::new(""),
            Change::Replace { new_text, .. } => new_text.clone(),
        }
    }

    pub fn range(&self) -> Range<usize> {
        match self {
            Change::Insert { range, .. } => range.clone(),
            Change::Delete { range, .. } => range.clone(),
            Change::Replace { range, .. } => range.clone(),
        }
    }

    pub fn selection_range(&self) -> Range<usize> {
        match self {
            Change::Insert { range, text } => range.start..range.start + text.len(),
            Change::Delete { range, .. } => range.start..range.start,
            Change::Replace {
                range, new_text, ..
            } => range.start..range.start + new_text.len(),
        }
    }

    fn merge_with(self, other: &Change) -> Option<Change> {
        use Change::*;

        match (self, other) {
            (
                Insert {
                    range: r1,
                    text: t1,
                },
                Insert {
                    range: r2,
                    text: t2,
                },
            ) if r1.start + t1.len() == r2.start => Some(Insert {
                range: r1.start..r1.start,
                text: SharedString::from(format!("{}{}", t1, t2)),
            }),
            (
                Delete {
                    range: r1,
                    text: t1,
                },
                Insert {
                    range: r2,
                    text: t2,
                },
            ) if r1.start == r2.start => Some(Replace {
                range: r1.clone(),
                old_text: t1,
                new_text: t2.clone(),
                marked: false,
            }),
            (
                Delete {
                    range: r1,
                    text: t1,
                },
                Delete {
                    range: r2,
                    text: t2,
                },
            ) => match (r1.start, r1.end, r2.start, r2.end) {
                (start1, end1, start2, end2) if start1 == end2 => Some(Delete {
                    range: start2..end1,
                    text: SharedString::from(format!("{}{}", t2, t1)),
                }),
                (start1, _, start2, _) if start1 == start2 => {
                    let text = SharedString::from(format!("{}{}", t1, t2));
                    Some(Delete {
                        range: start1..start1 + text.len(),
                        text,
                    })
                }
                _ => None,
            },
            (
                Replace {
                    range: r1,
                    new_text: t1,
                    old_text,
                    ..
                },
                Insert { text: t2, .. },
            ) => Some(Replace {
                range: r1,
                new_text: SharedString::from(format!("{}{}", t1, t2)),
                old_text,
                marked: false,
            }),
            (
                Insert {
                    text: t1,
                    range: r1,
                },
                Replace {
                    new_text,
                    old_text,
                    marked: true,
                    ..
                },
            ) if t1.ends_with(old_text.as_ref()) => Some(Insert {
                range: r1,
                text: SharedString::from(format!(
                    "{}{}",
                    &t1[..t1.len() - old_text.len()],
                    new_text
                )),
            }),
            (
                Replace {
                    range: r1,
                    new_text: nt1,
                    old_text: ot1,
                    ..
                },
                Replace {
                    new_text: nt2,
                    old_text: ot2,
                    marked: true,
                    ..
                },
            ) if nt1.ends_with(ot2.as_ref()) => Some(Replace {
                range: r1,
                old_text: ot1,
                new_text: SharedString::from(format!("{}{}", &nt1[..nt1.len() - ot2.len()], nt2)),
                marked: false,
            }),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct HistoryEntry {
    pub change: Change,
}

pub struct History {
    undo_stack: VecDeque<HistoryEntry>,
    redo_stack: VecDeque<HistoryEntry>,
    max_size: usize,
    can_merge: bool,
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

impl History {
    pub fn new() -> Self {
        Self::with_max_size(100)
    }

    pub fn with_max_size(max_size: usize) -> Self {
        Self {
            undo_stack: VecDeque::new(),
            redo_stack: VecDeque::new(),
            max_size,
            can_merge: true,
        }
    }

    pub fn push(&mut self, change: Change) {
        self.redo_stack.clear();

        if self.can_merge
            && let Some(last_entry) = self.undo_stack.back_mut()
            && let Some(merged_change) = last_entry.change.clone().merge_with(&change)
        {
            last_entry.change = merged_change;
            return;
        }

        self.undo_stack.push_back(HistoryEntry { change });
        if self.undo_stack.len() > self.max_size {
            self.undo_stack.pop_front();
        }
        self.can_merge = true;
    }

    pub fn undo(&mut self) -> Option<Change> {
        self.prevent_merge();
        if let Some(entry) = self.undo_stack.pop_back() {
            self.redo_stack.push_back(entry.clone());
            let inverse_change = entry.change.inverse();
            Some(inverse_change)
        } else {
            None
        }
    }

    pub fn redo(&mut self) -> Option<Change> {
        self.prevent_merge();
        if let Some(entry) = self.redo_stack.pop_back() {
            self.undo_stack.push_back(entry.clone());
            Some(entry.change)
        } else {
            None
        }
    }

    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    pub fn prevent_merge(&mut self) {
        self.can_merge = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ops::Range;

    struct Editor {
        value: String,
        selection: Range<usize>,
        marked: Option<Range<usize>>,
        history: History,
    }

    impl Editor {
        fn new(text: &str) -> Self {
            Self {
                value: text.to_string(),
                selection: text.len()..text.len(),
                marked: None,
                history: History::new(),
            }
        }

        fn select(&mut self, range: Range<usize>) {
            self.history.prevent_merge();
            self.selection = range;
        }

        fn type_text(&mut self, text: &str) {
            for ch in text.chars() {
                self.replace(self.selection.clone(), ch.encode_utf8(&mut [0; 4]));
            }
        }

        fn backspace(&mut self) {
            if self.selection.is_empty() {
                let end = self.selection.end;
                let start = self.value[..end]
                    .chars()
                    .next_back()
                    .map_or(end, |ch| end - ch.len_utf8());
                self.selection = start..end;
            }
            self.replace(self.selection.clone(), "");
        }

        fn delete(&mut self) {
            if self.selection.is_empty() {
                let start = self.selection.start;
                let end = self.value[start..]
                    .chars()
                    .next()
                    .map_or(start, |ch| start + ch.len_utf8());
                self.selection = start..end;
            }
            self.replace(self.selection.clone(), "");
        }

        fn cut(&mut self) {
            self.history.prevent_merge();
            self.replace(self.selection.clone(), "");
        }

        fn paste(&mut self, text: &str) {
            self.history.prevent_merge();
            self.replace(self.selection.clone(), text);
        }

        fn compose(&mut self, text: &str) {
            let range = self.marked.clone().unwrap_or(self.selection.clone());
            self.replace(range.clone(), text);
            self.marked = Some(range.start..range.start + text.len());
        }

        fn commit(&mut self, text: &str) {
            let range = self.marked.clone().unwrap_or(self.selection.clone());
            self.replace(range, text);
            self.marked = None;
        }

        #[track_caller]
        fn undo(&mut self) -> String {
            self.try_undo().expect("there should be something to undo")
        }

        fn try_undo(&mut self) -> Option<String> {
            let change = self.history.undo()?;
            self.apply(&change);
            self.selection = change.selection_range();
            Some(self.render())
        }

        #[track_caller]
        fn redo(&mut self) -> String {
            self.try_redo().expect("there should be something to redo")
        }

        fn try_redo(&mut self) -> Option<String> {
            let change = self.history.redo()?;
            self.apply(&change);
            Some(self.render())
        }

        fn replace(&mut self, range: Range<usize>, text: &str) {
            let change = if range.is_empty() {
                Change::Insert {
                    range: range.clone(),
                    text: text.to_string().into(),
                }
            } else if text.is_empty() {
                Change::Delete {
                    range: range.clone(),
                    text: self.value[range.clone()].to_string().into(),
                }
            } else {
                Change::Replace {
                    range: range.clone(),
                    old_text: self.value[range.clone()].to_string().into(),
                    new_text: text.to_string().into(),
                    marked: self.marked.is_some(),
                }
            };
            self.history.push(change);

            self.value.replace_range(range.clone(), text);
            let cursor = range.start + text.len();
            self.selection = cursor..cursor;
        }

        fn apply(&mut self, change: &Change) {
            self.value.replace_range(change.range(), &change.text());
            let cursor = change.range().start + change.text().len();
            self.selection = cursor..cursor;
            self.marked = None;
        }

        /// Renders the value with `|` at the cursor or `[...]` around the selection.
        fn render(&self) -> String {
            let Range { start, end } = self.selection.clone();
            if start == end {
                format!("{}|{}", &self.value[..start], &self.value[start..])
            } else {
                format!(
                    "{}[{}]{}",
                    &self.value[..start],
                    &self.value[start..end],
                    &self.value[end..]
                )
            }
        }
    }

    fn strip_selection(rendered: &str) -> String {
        rendered.replace(['|', '[', ']'], "")
    }

    /// Starts from `initial` with an empty history and runs `edit`. Then undoes everything,
    /// checking the state after each step, and redoes everything.
    #[track_caller]
    fn check(initial: &str, edit: impl FnOnce(&mut Editor), undo_states: &[&str]) {
        let mut editor = Editor::new(initial);
        edit(&mut editor);
        let edited = editor.value.clone();

        for expected in undo_states {
            assert_eq!(editor.undo(), *expected);
        }
        assert_eq!(editor.try_undo(), None, "undo history should be exhausted");

        let redo_values = undo_states
            .iter()
            .rev()
            .skip(1)
            .map(|state| strip_selection(state))
            .chain([edited]);
        for expected in redo_values {
            editor.redo();
            assert_eq!(editor.value, expected);
        }
        assert_eq!(editor.try_redo(), None, "redo history should be exhausted");
    }

    #[test]
    fn type_undoes_as_one_step() {
        check("", |e| e.type_text("Hello World!"), &["|"]);
    }

    #[test]
    fn type_over_selection_undoes_as_one_step() {
        check(
            "",
            |e| {
                e.type_text("abcdef");
                e.select(2..4);
                e.type_text("XY");
            },
            &["ab[cd]ef", "|"],
        );

        check(
            "",
            |e| {
                e.type_text("important note");
                e.select(0..14);
                e.type_text("REMOVED");
            },
            &["[important note]", "|"],
        );
    }

    #[test]
    fn cut_and_paste_undo_separately() {
        check(
            "",
            |e| {
                e.type_text("quick brown fox");
                e.select(6..12);
                e.cut();
                e.select(0..0);
                e.paste("brown ");
            },
            &["|quick fox", "quick [brown ]fox", "|"],
        );
    }

    #[test]
    fn repeated_backspaces_merge_on_undo() {
        check(
            "abcdef",
            |e| {
                e.select(4..4);
                e.backspace();
                e.backspace();
            },
            &["ab[cd]ef"],
        );
    }

    #[test]
    fn repeated_forward_deletes_merge_on_undo() {
        check(
            "abcdef",
            |e| {
                e.select(2..2);
                e.delete();
                e.delete();
            },
            &["ab[cd]ef"],
        );
    }

    #[test]
    fn delete_and_retype_undoes_as_replacement() {
        check(
            "",
            |e| {
                e.type_text("world");
                e.backspace();
                e.backspace();
                e.type_text("ldwide");
            },
            &["wor[ld]", "|"],
        );
    }

    #[test]
    fn multibyte_chars_undo_on_char_boundaries() {
        check(
            "",
            |e| {
                e.type_text("hello 👋 world");
                e.select(6..10);
                e.paste("🌍");
            },
            &["hello [👋] world", "|"],
        );
    }

    #[test]
    fn composed_char_undoes_as_one_step() {
        check(
            "",
            |e| {
                e.compose("´");
                e.commit("á");
            },
            &["|"],
        );
    }

    #[test]
    fn composing_merges_with_typing() {
        check(
            "",
            |e| {
                e.type_text("hello w");
                e.compose("´");
                e.commit("ó");
                e.type_text("rld");
            },
            &["|"],
        );
    }

    #[test]
    fn compose_over_selection_undoes_as_replacement() {
        check(
            "",
            |e| {
                e.type_text("hello fucking world");
                e.select(6..13);
                e.compose("´");
                e.commit("á");
                e.type_text("wesome");
            },
            &["hello [fucking] world", "|"],
        );
    }

    #[test]
    fn undo_and_redo_toggle_the_same_edit() {
        let mut editor = Editor::new("Hello World!");
        editor.select(0..5);
        editor.cut();

        assert_eq!(editor.undo(), "[Hello] World!");
        assert_eq!(editor.redo(), "| World!");
        assert_eq!(editor.undo(), "[Hello] World!");
    }

    #[test]
    fn new_edit_after_undo_discards_redo_history() {
        let mut editor = Editor::new("tree");
        editor.select(2..2);
        editor.backspace();
        assert_eq!(editor.undo(), "t[r]ee");

        editor.select(4..4);
        editor.type_text("s");
        assert_eq!(editor.try_redo(), None);
    }
}
