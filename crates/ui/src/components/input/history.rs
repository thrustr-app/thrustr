// SPDX-License-Identifier: GPL-3.0-or-later
//
// Adapted from gpui-kit,
// Copyright (C) Longbridge, licensed under Apache-2.0:
// https://github.com/longbridge/gpui-kit/blob/main/crates/base/src/history.rs
//
// Modified and redistributed as part of Thrustr under GPL-3.0-or-later.

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
                (start1, end1, start2, end2) if start1 == start2 => Some(Delete {
                    range: start1..end1.max(end2),
                    text: SharedString::from(format!("{}{}", t1, t2)),
                }),
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

    fn insert_text(history: &mut History, text: &str) {
        for (i, ch) in text.char_indices() {
            history.push(Change::Insert {
                text: ch.to_string().into(),
                range: i..i,
            });
        }
    }

    fn cut_text(history: &mut History, text: &str, range: Range<usize>) {
        history.prevent_merge();
        history.push(Change::Delete {
            text: text.to_string().into(),
            range,
        });
    }

    fn paste_text(history: &mut History, text: &str, range: Range<usize>) {
        history.prevent_merge();
        history.push(Change::Insert {
            text: text.to_string().into(),
            range,
        });
    }

    #[test]
    fn simple_insertions() {
        let mut history = History::new();
        insert_text(&mut history, "Hello World!");

        let undo = history.undo().unwrap();
        assert_eq!(
            undo,
            Change::Delete {
                text: "".into(),
                range: 0..12
            }
        );
        assert_eq!(undo.selection_range(), 0..0);

        let redo = history.redo().unwrap();
        assert_eq!(
            redo,
            Change::Insert {
                text: "Hello World!".into(),
                range: 0..0
            }
        );
    }

    #[test]
    fn paste_over_selection() {
        let mut history = History::new();
        insert_text(&mut history, "abcdef");

        history.push(Change::Replace {
            range: 2..4,
            old_text: "cd".into(),
            new_text: "X".into(),
            marked: false,
        });
        history.push(Change::Insert {
            text: "Y".into(),
            range: 3..3,
        });

        let undo = history.undo().unwrap();
        assert_eq!(
            undo,
            Change::Replace {
                range: 2..4,
                old_text: "XY".into(),
                new_text: "cd".into(),
                marked: false,
            }
        );
        assert_eq!(undo.selection_range(), 2..4);

        let undo = history.undo().unwrap();
        assert_eq!(
            undo,
            Change::Delete {
                text: "".into(),
                range: 0..6
            }
        );
        assert_eq!(undo.selection_range(), 0..0);

        let redo = history.redo().unwrap();
        assert_eq!(
            redo,
            Change::Insert {
                text: "abcdef".into(),
                range: 0..0
            }
        );

        let redo = history.redo().unwrap();
        assert_eq!(
            redo,
            Change::Replace {
                range: 2..4,
                old_text: "cd".into(),
                new_text: "XY".into(),
                marked: false,
            }
        );
    }

    #[test]
    fn cut_and_paste() {
        let mut history = History::new();
        insert_text(&mut history, "quick brown fox");
        cut_text(&mut history, "brown ", 6..12);
        paste_text(&mut history, "brown ", 0..0);

        let undo = history.undo().unwrap();
        assert_eq!(
            undo,
            Change::Delete {
                text: "".into(),
                range: 0..6
            }
        );
        assert_eq!(undo.selection_range(), 0..0);

        let undo = history.undo().unwrap();
        assert_eq!(
            undo,
            Change::Insert {
                text: "brown ".into(),
                range: 6..6
            }
        );
        assert_eq!(undo.selection_range(), 6..12);

        let undo = history.undo().unwrap();
        assert_eq!(
            undo,
            Change::Delete {
                text: "".into(),
                range: 0..15
            }
        );
        assert_eq!(undo.selection_range(), 0..0);

        let redo = history.redo().unwrap();
        assert_eq!(
            redo,
            Change::Insert {
                text: "quick brown fox".into(),
                range: 0..0
            }
        );

        let redo = history.redo().unwrap();
        assert_eq!(
            redo,
            Change::Delete {
                text: "brown ".into(),
                range: 6..12
            }
        );

        let redo = history.redo().unwrap();
        assert_eq!(
            redo,
            Change::Insert {
                text: "brown ".into(),
                range: 0..0
            }
        );
    }

    #[test]
    fn replace_same_text() {
        let mut history = History::new();
        insert_text(&mut history, "quick brown fox");

        history.push(Change::Replace {
            range: 6..11,
            old_text: "brown".into(),
            new_text: "brown".into(),
            marked: false,
        });

        let undo = history.undo().unwrap();
        assert_eq!(
            undo,
            Change::Replace {
                range: 6..11,
                old_text: "brown".into(),
                new_text: "brown".into(),
                marked: false,
            }
        );
        assert_eq!(undo.selection_range(), 6..11);

        let undo = history.undo().unwrap();
        assert_eq!(
            undo,
            Change::Delete {
                text: "".into(),
                range: 0..15
            }
        );
        assert_eq!(undo.selection_range(), 0..0);

        let redo = history.redo().unwrap();
        assert_eq!(
            redo,
            Change::Insert {
                text: "quick brown fox".into(),
                range: 0..0
            }
        );
    }

    #[test]
    fn undo_redo_mixed() {
        let mut history = History::new();
        insert_text(&mut history, "Hello World!");
        cut_text(&mut history, "Hello", 0..5);

        let undo = history.undo().unwrap();
        assert_eq!(
            undo,
            Change::Insert {
                text: "Hello".into(),
                range: 0..0
            }
        );
        assert_eq!(undo.selection_range(), 0..5);

        let redo = history.redo().unwrap();
        assert_eq!(
            redo,
            Change::Delete {
                range: 0..5,
                text: "Hello".into(),
            }
        );

        let undo = history.undo().unwrap();
        assert_eq!(
            undo,
            Change::Insert {
                text: "Hello".into(),
                range: 0..0
            }
        );
        assert_eq!(undo.selection_range(), 0..5);
    }

    #[test]
    fn undo_clear() {
        let mut history = History::new();
        insert_text(&mut history, "tree");

        history.push(Change::Delete {
            text: "r".into(),
            range: 1..2,
        });

        let undo = history.undo().unwrap();
        assert_eq!(
            undo,
            Change::Insert {
                text: "r".into(),
                range: 1..1
            }
        );
        assert_eq!(undo.selection_range(), 1..2);

        history.push(Change::Insert {
            text: "s".into(),
            range: 4..4,
        });

        assert!(history.redo().is_none())
    }

    #[test]
    fn write_delete_type() {
        let mut history = History::new();
        insert_text(&mut history, "world");

        history.push(Change::Delete {
            text: "ld".into(),
            range: 3..5,
        });

        history.push(Change::Insert {
            range: 3..3,
            text: "l".into(),
        });
        history.push(Change::Insert {
            range: 4..4,
            text: "d".into(),
        });
        history.push(Change::Insert {
            range: 5..5,
            text: "w".into(),
        });
        history.push(Change::Insert {
            range: 6..6,
            text: "i".into(),
        });
        history.push(Change::Insert {
            range: 7..7,
            text: "d".into(),
        });
        history.push(Change::Insert {
            range: 8..8,
            text: "e".into(),
        });

        let undo = history.undo().unwrap();
        assert_eq!(
            undo,
            Change::Replace {
                old_text: "ldwide".into(),
                new_text: "ld".into(),
                range: 3..9,
                marked: false,
            }
        );
        assert_eq!(undo.selection_range(), 3..5);

        let undo = history.undo().unwrap();
        assert_eq!(
            undo,
            Change::Delete {
                text: "".into(),
                range: 0..5
            }
        );
        assert_eq!(undo.selection_range(), 0..0);

        let redo = history.redo().unwrap();
        assert_eq!(
            redo,
            Change::Insert {
                text: "world".into(),
                range: 0..0
            }
        );

        let redo = history.redo().unwrap();
        assert_eq!(
            redo,
            Change::Replace {
                old_text: "ld".into(),
                new_text: "ldwide".into(),
                range: 3..5,
                marked: false,
            }
        );
    }

    #[test]
    fn select_all_replace() {
        let mut history = History::new();
        insert_text(&mut history, "important note");

        history.push(Change::Replace {
            range: 0..14,
            old_text: "important note".into(),
            new_text: "R".into(),
            marked: false,
        });

        history.push(Change::Insert {
            range: 1..1,
            text: "E".into(),
        });
        history.push(Change::Insert {
            range: 2..2,
            text: "M".into(),
        });
        history.push(Change::Insert {
            range: 3..3,
            text: "O".into(),
        });
        history.push(Change::Insert {
            range: 4..4,
            text: "V".into(),
        });
        history.push(Change::Insert {
            range: 5..5,
            text: "E".into(),
        });
        history.push(Change::Insert {
            range: 6..6,
            text: "D".into(),
        });

        let undo = history.undo().unwrap();
        assert_eq!(
            undo,
            Change::Replace {
                old_text: "REMOVED".into(),
                new_text: "important note".into(),
                range: 0..7,
                marked: false,
            }
        );
        assert_eq!(undo.selection_range(), 0..14);

        let undo = history.undo().unwrap();
        assert_eq!(
            undo,
            Change::Delete {
                text: "".into(),
                range: 0..14
            }
        );
        assert_eq!(undo.selection_range(), 0..0);

        let redo = history.redo().unwrap();
        assert_eq!(
            redo,
            Change::Insert {
                text: "important note".into(),
                range: 0..0
            }
        );

        let redo = history.redo().unwrap();
        assert_eq!(
            redo,
            Change::Replace {
                old_text: "important note".into(),
                new_text: "REMOVED".into(),
                range: 0..14,
                marked: false,
            }
        );
    }

    #[test]
    fn emojis() {
        let mut history = History::new();
        insert_text(&mut history, "hello 👋 world");

        history.push(Change::Replace {
            range: 6..10,
            old_text: "👋".into(),
            new_text: "🌍".into(),
            marked: false,
        });

        let undo = history.undo().unwrap();
        assert_eq!(
            undo,
            Change::Replace {
                range: 6..10,
                old_text: "🌍".into(),
                new_text: "👋".into(),
                marked: false,
            }
        );
        assert_eq!(undo.selection_range(), 6..10);

        let undo = history.undo().unwrap();
        assert_eq!(
            undo,
            Change::Delete {
                text: "".into(),
                range: 0..16
            }
        );
        assert_eq!(undo.selection_range(), 0..0);

        let redo = history.redo().unwrap();
        assert_eq!(
            redo,
            Change::Insert {
                text: "hello 👋 world".into(),
                range: 0..0
            }
        );

        let redo = history.redo().unwrap();
        assert_eq!(
            redo,
            Change::Replace {
                range: 6..10,
                old_text: "👋".into(),
                new_text: "🌍".into(),
                marked: false,
            }
        );
    }

    #[test]
    fn simple_marked() {
        let mut history = History::new();

        history.push(Change::Insert {
            range: 0..0,
            text: "´".into(),
        });

        history.push(Change::Replace {
            range: 0..2,
            old_text: "´".into(),
            new_text: "á".into(),
            marked: true,
        });

        let undo = history.undo().unwrap();
        assert_eq!(
            undo,
            Change::Delete {
                text: "".into(),
                range: 0..2
            }
        );
        assert_eq!(undo.selection_range(), 0..0);

        let redo = history.redo().unwrap();
        assert_eq!(
            redo,
            Change::Insert {
                text: "á".into(),
                range: 0..0
            }
        );
    }

    #[test]
    fn marked_sequence() {
        let mut history = History::new();
        insert_text(&mut history, "hello w´");

        history.push(Change::Replace {
            range: 7..9,
            old_text: "´".into(),
            new_text: "ó".into(),
            marked: true,
        });
        history.push(Change::Insert {
            range: 9..9,
            text: "rld".into(),
        });

        let undo = history.undo().unwrap();
        assert_eq!(
            undo,
            Change::Delete {
                range: 0..12,
                text: "".into()
            }
        );
        assert_eq!(undo.selection_range(), 0..0);

        let redo = history.redo().unwrap();
        assert_eq!(
            redo,
            Change::Insert {
                text: "hello wórld".into(),
                range: 0..0
            }
        );
    }

    #[test]
    fn marked_replace_sequence() {
        let mut history = History::new();
        insert_text(&mut history, "hello fucking world");

        history.push(Change::Replace {
            range: 6..13,
            old_text: "fucking".into(),
            new_text: "´".into(),
            marked: false,
        });
        history.push(Change::Replace {
            range: 6..8,
            old_text: "´".into(),
            new_text: "á".into(),
            marked: true,
        });
        history.push(Change::Insert {
            range: 8..8,
            text: "wesome".into(),
        });

        let undo = history.undo().unwrap();
        assert_eq!(
            undo,
            Change::Replace {
                range: 6..14,
                old_text: "áwesome".into(),
                new_text: "fucking".into(),
                marked: false,
            }
        );
        assert_eq!(undo.selection_range(), 6..13);

        let undo = history.undo().unwrap();
        assert_eq!(
            undo,
            Change::Delete {
                range: 0..19,
                text: "".into()
            }
        );
        assert_eq!(undo.selection_range(), 0..0);

        let redo = history.redo().unwrap();
        assert_eq!(
            redo,
            Change::Insert {
                range: 0..0,
                text: "hello fucking world".into()
            }
        );

        let redo = history.redo().unwrap();
        assert_eq!(
            redo,
            Change::Replace {
                range: 6..13,
                old_text: "fucking".into(),
                new_text: "áwesome".into(),
                marked: false,
            }
        );
    }
}
