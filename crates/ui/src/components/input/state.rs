// SPDX-License-Identifier: GPL-3.0-or-later
//
// Parts of this module are adapted from gpui-kit,
// Copyright (C) Longbridge, licensed under Apache-2.0:
// https://github.com/longbridge/gpui-kit/blob/main/crates/base/src/input/base/mod.rs
//
// Parts of this module are adapted from the gpui text-input example,
// Copyright (C) Zed Industries, Inc., licensed under Apache-2.0:
// https://github.com/zed-industries/zed/blob/main/crates/gpui/examples/input.rs
//
// Modified and redistributed as part of Thrustr under GPL-3.0-or-later.

use crate::components::input::{
    actions::*,
    cursor::Cursor,
    element::{CURSOR_WIDTH, TextElement},
    history::{Change, History},
    text_ops::TextOps,
    *,
};
use gpui::*;
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

const DEFAULT_MASK: &str = "•";
const DEFAULT_SELECTION_COLOR: u32 = 0x3390FF80;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputEvent {
    Input,
    Change,
}

/// State of a single-line text field, owned by the view rendering it.
///
/// Handles text editing, cursor positioning, selection, and scrolling, and
/// emits [`InputEvent`]s as the user edits the text.
pub struct InputState {
    pub(super) focus_handle: FocusHandle,
    pub(super) value: SharedString,
    pub(super) committed_value: SharedString,
    pub(super) placeholder: SharedString,
    /// Falls back to the theme's placeholder color.
    pub(super) placeholder_color: Option<Hsla>,
    pub(super) selection_color: Hsla,
    pub(super) selected_range: Range<usize>,
    pub(super) selection_reversed: bool,
    pub(super) marked_range: Option<Range<usize>>,
    pub(super) last_layout: Option<ShapedLine>,
    pub(super) last_bounds: Option<Bounds<Pixels>>,
    pub(super) selecting: bool,
    pub(super) scroll_handle: ScrollHandle,
    pub(super) should_auto_scroll: bool,
    pub(super) cursor: Entity<Cursor>,
    pub(super) masked: bool,
    pub(super) mask: SharedString,
    pub(super) max_length: Option<usize>,
    history: History,
    ignore_history: bool,
    focus_select: bool,
    _subscriptions: [Subscription; 4],
}

impl InputState {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let cursor = cx.new(|_| Cursor::new());
        let focus_handle = cx.focus_handle();

        let _subscriptions = [
            cx.observe(&cursor, |state, _, cx| {
                if !state.selecting {
                    cx.notify();
                }
            }),
            cx.observe_window_activation(window, |state, window, cx| {
                let active = window.is_window_active();
                let focused = state.focus_handle.is_focused(window);
                state.cursor.update(cx, |cursor, cx| {
                    if !active {
                        cursor.stop();
                    } else if focused {
                        cursor.start(cx);
                    }
                });
            }),
            cx.on_focus(&focus_handle, window, |_, window, cx| {
                cx.defer_in(window, Self::on_focus)
            }),
            cx.on_blur(&focus_handle, window, |_, window, cx| {
                cx.defer_in(window, Self::on_blur)
            }),
        ];

        Self {
            focus_handle,
            value: SharedString::default(),
            committed_value: SharedString::default(),
            placeholder: SharedString::default(),
            placeholder_color: None,
            selection_color: rgba(DEFAULT_SELECTION_COLOR).into(),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            selecting: false,
            scroll_handle: ScrollHandle::new(),
            should_auto_scroll: false,
            masked: false,
            mask: SharedString::new(DEFAULT_MASK),
            max_length: None,
            history: History::new(),
            ignore_history: false,
            focus_select: true,
            cursor,
            _subscriptions,
        }
    }

    pub fn with_placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn set_placeholder(
        &mut self,
        placeholder: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) {
        self.placeholder = placeholder.into();
        cx.notify();
    }

    pub fn with_placeholder_color(mut self, color: impl Into<Hsla>) -> Self {
        self.placeholder_color = Some(color.into());
        self
    }

    pub fn with_selection_color(mut self, color: impl Into<Hsla>) -> Self {
        self.selection_color = color.into();
        self
    }

    pub fn with_masked(mut self, masked: bool) -> Self {
        self.masked = masked;
        self
    }

    pub fn set_masked(&mut self, masked: bool, cx: &mut Context<Self>) {
        if self.masked != masked {
            self.masked = masked;
            self.should_auto_scroll = true;
            cx.notify();
        }
    }

    pub fn with_mask(mut self, mask: impl Into<SharedString>) -> Self {
        self.mask = mask.into();
        self
    }

    pub fn with_max_length(mut self, max_length: usize) -> Self {
        self.max_length = Some(max_length);
        self
    }

    pub fn with_value(mut self, value: impl Into<SharedString>) -> Self {
        self.replace_value(value.into());
        self
    }

    pub fn value(&self) -> &SharedString {
        &self.value
    }

    /// Replace the text, dropping the undo history and placing the cursor at
    /// the end. Unlike user edits, this emits no [`InputEvent`].
    pub fn set_value(&mut self, value: impl Into<SharedString>, cx: &mut Context<Self>) {
        let value = value.into();
        if value != self.value {
            self.replace_value(value);
            cx.notify();
        }
    }

    /// Replace the text without notifying.
    pub(crate) fn replace_value(&mut self, value: SharedString) {
        self.selected_range = value.len()..value.len();
        self.selection_reversed = false;
        self.marked_range = None;
        self.committed_value = value.clone();
        self.value = value;
        self.history.clear();
        self.should_auto_scroll = true;
    }

    /// Replace the placeholder without notifying.
    pub(crate) fn replace_placeholder(&mut self, placeholder: SharedString) {
        self.placeholder = placeholder;
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.focus_handle.focus(window, cx);
    }

    /// Clear the text as a user edit.
    pub(super) fn clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.value.is_empty() {
            self.selected_range = 0..self.value.len();
            self.marked_range = None;
            self.replace_text_in_range(None, "", window, cx);
        }
    }

    fn on_focus(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.focus_select {
            self.selected_range = 0..self.value.len();
            cx.notify();
        }
        self.cursor.update(cx, |cursor, cx| {
            cursor.start(cx);
        });
        self.focus_select = true;
    }

    fn on_blur(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.focus_handle.is_focused(window) {
            self.selected_range = 0..0;
            self.history.prevent_merge();
        }
        self.cursor.update(cx, |cursor, _| {
            cursor.stop();
        });
        cx.notify();
        self.commit(cx);
    }

    fn commit(&mut self, cx: &mut Context<Self>) {
        if self.value != self.committed_value {
            self.committed_value = self.value.clone();
            cx.emit(InputEvent::Change);
        }
    }

    fn pause_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.cursor.update(cx, |cursor, cx| {
            cursor.pause(cx);
        });
    }

    pub(super) fn cursor_visible(&self, window: &Window, cx: &App) -> bool {
        self.focus_handle.is_focused(window) && self.cursor.read(cx).visible()
    }

    pub(super) fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(
                TextOps::previous_boundary(&self.value, self.cursor_offset()),
                cx,
            );
        } else {
            self.move_to(self.selected_range.start, cx);
        }
    }

    pub(super) fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(
                TextOps::next_boundary(&self.value, self.selected_range.end),
                cx,
            );
        } else {
            self.move_to(self.selected_range.end, cx);
        }
    }

    pub(super) fn word_left(&mut self, _: &WordLeft, _: &mut Window, cx: &mut Context<Self>) {
        let new_offset = TextOps::previous_word_boundary(&self.value, self.cursor_offset());
        self.move_to(new_offset, cx);
    }

    pub(super) fn word_right(&mut self, _: &WordRight, _: &mut Window, cx: &mut Context<Self>) {
        let new_offset = TextOps::next_word_boundary(&self.value, self.cursor_offset());
        self.move_to(new_offset, cx);
    }

    pub(super) fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    pub(super) fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.value.len(), cx);
    }

    pub(super) fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.pause_cursor_blink(cx);
        let offset = offset.clamp(0, self.value.len());
        if offset != self.cursor_offset() {
            self.should_auto_scroll = true;
            self.history.prevent_merge();
        }

        self.selected_range = offset..offset;
        cx.notify();
    }

    pub(super) fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(
            TextOps::previous_boundary(&self.value, self.cursor_offset()),
            cx,
        );
    }

    pub(super) fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(
            TextOps::next_boundary(&self.value, self.cursor_offset()),
            cx,
        );
    }

    pub(super) fn select_word_left(
        &mut self,
        _: &SelectWordLeft,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let new_offset = TextOps::previous_word_boundary(&self.value, self.cursor_offset());
        self.history.prevent_merge();
        self.select_to(new_offset, cx);
    }

    pub(super) fn select_word_right(
        &mut self,
        _: &SelectWordRight,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let new_offset = TextOps::next_word_boundary(&self.value, self.cursor_offset());
        self.history.prevent_merge();
        self.select_to(new_offset, cx);
    }

    pub(super) fn select_to_beginning(
        &mut self,
        _: &SelectToBeginning,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_to(0, cx);
    }

    pub(super) fn select_to_end(
        &mut self,
        _: &SelectToEnd,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_to(self.value.len(), cx);
    }

    pub(super) fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.value.len(), cx);
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }

        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }

        self.should_auto_scroll = true;
        cx.notify();
    }

    fn select_word(&mut self, offset: usize, cx: &mut Context<Self>) {
        let start = TextOps::previous_word_boundary(&self.value, offset);
        let end = TextOps::next_word_boundary(&self.value, offset);
        self.selected_range = start..end;
        self.selection_reversed = false;
        cx.notify();
    }

    pub(super) fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(
                TextOps::previous_boundary(&self.value, self.cursor_offset()),
                cx,
            );
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    pub(super) fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(
                TextOps::next_boundary(&self.value, self.cursor_offset()),
                cx,
            );
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    pub(super) fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.history.prevent_merge();
            self.replace_text_in_range(None, &text.replace('\n', " "), window, cx);
        }
    }

    pub(super) fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            let selected_text = self.value[self.selected_range.clone()].to_string();
            cx.write_to_clipboard(ClipboardItem::new_string(selected_text));
        }
    }

    pub(super) fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            let selected_text = self.value[self.selected_range.clone()].to_string();
            cx.write_to_clipboard(ClipboardItem::new_string(selected_text));
            self.history.prevent_merge();
            self.replace_text_in_range(None, "", window, cx);
        }
    }

    pub(super) fn undo(&mut self, _: &Undo, window: &mut Window, cx: &mut Context<Self>) {
        self.ignore_history = true;

        if let Some(change) = self.history.undo() {
            self.replace_text_in_range(
                Some(TextOps::range_to_utf16(&self.value, &change.range())),
                &change.text(),
                window,
                cx,
            );
            self.selected_range = change.selection_range();
        }
        self.ignore_history = false;
    }

    pub(super) fn redo(&mut self, _: &Redo, window: &mut Window, cx: &mut Context<Self>) {
        self.ignore_history = true;
        if let Some(change) = self.history.redo() {
            self.replace_text_in_range(
                Some(TextOps::range_to_utf16(&self.value, &change.range())),
                &change.text(),
                window,
                cx,
            );
        }
        self.ignore_history = false;
    }

    fn push_history(&mut self, new_text: &str, range: &Range<usize>) {
        if self.ignore_history {
            return;
        }

        if range.start == 0 && range.end == 0 && new_text.is_empty() {
            return;
        }

        let marked = self.marked_range.is_some();

        if range.start == range.end {
            self.history.push(Change::Insert {
                range: range.clone(),
                text: new_text.to_string().into(),
            });
        } else if new_text.is_empty() {
            self.history.push(Change::Delete {
                range: range.clone(),
                text: self.value[range.start..range.end].to_string().into(),
            })
        } else {
            self.history.push(Change::Replace {
                range: range.clone(),
                new_text: new_text.to_string().into(),
                old_text: self.value[range.start..range.end].to_string().into(),
                marked,
            });
        }
    }

    pub(super) fn delete_word_left(
        &mut self,
        _: &DeleteWordLeft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_range.is_empty() {
            let cursor_pos = self.cursor_offset();
            let word_start = TextOps::previous_word_boundary(&self.value, cursor_pos);
            self.selected_range = word_start..cursor_pos;
        }
        self.history.prevent_merge();
        self.replace_text_in_range(None, "", window, cx);
    }

    pub(super) fn delete_word_right(
        &mut self,
        _: &DeleteWordRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_range.is_empty() {
            let cursor_pos = self.cursor_offset();
            let word_end = TextOps::next_word_boundary(&self.value, cursor_pos);
            self.selected_range = cursor_pos..word_end;
        }
        self.history.prevent_merge();
        self.replace_text_in_range(None, "", window, cx);
    }

    pub(super) fn delete_to_beginning(
        &mut self,
        _: &DeleteToBeginning,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_range.is_empty() {
            let cursor_pos = self.cursor_offset();
            self.selected_range = 0..cursor_pos;
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    pub(super) fn delete_to_end(
        &mut self,
        _: &DeleteToEnd,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_range.is_empty() {
            let cursor_pos = self.cursor_offset();
            self.selected_range = cursor_pos..self.value.len();
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    pub(super) fn enter(&mut self, _: &Enter, _: &mut Window, cx: &mut Context<Self>) {
        self.commit(cx);
        cx.propagate();
    }

    pub(super) fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selecting = true;
        self.focus_select = false;

        if event.click_count > 1 {
            if event.click_count.is_multiple_of(2) {
                self.select_word(self.index_for_mouse_position(event.position), cx);
            } else {
                self.select_all(&SelectAll, window, cx);
            }
            return;
        }

        let mouse_offset = self.index_for_mouse_position(event.position);
        if event.modifiers.shift {
            self.select_to(mouse_offset, cx);
        } else {
            self.move_to(mouse_offset, cx);
        }
    }

    pub(super) fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.selecting = false;
    }

    pub(super) fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selecting {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    pub(super) fn show_character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    pub(super) fn on_scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let delta = event.delta.pixel_delta(window.line_height());

        // Only consume horizontal wheel events.
        if delta.x.abs() <= delta.y.abs() {
            return;
        }
        cx.stop_propagation();

        let current_offset = self.scroll_handle.offset();
        let new_offset = current_offset - delta;
        self.update_scroll_offset(Some(new_offset), cx);
    }

    fn update_scroll_offset(&mut self, offset: Option<Point<Pixels>>, cx: &mut Context<Self>) {
        let mut offset = offset.unwrap_or(self.scroll_handle.offset());

        if let (Some(layout), Some(bounds)) = (self.last_layout.as_ref(), self.last_bounds.as_ref())
        {
            let text_width = layout.width;
            let visible_width = bounds.size.width - px(CURSOR_WIDTH);

            offset.x = offset.x.max(px(0.0));

            if text_width > visible_width {
                offset.x = offset.x.min(text_width - visible_width);
            } else {
                offset.x = px(0.0);
            }
        } else {
            offset.x = offset.x.max(px(0.0));
        }

        offset.y = px(0.0);

        self.scroll_handle.set_offset(offset);
        cx.notify();
    }

    pub(super) fn auto_scroll_to_cursor(&mut self, layout: &ShapedLine, bounds: Bounds<Pixels>) {
        self.should_auto_scroll = false;

        let cursor_offset = self.display_cursor_offset();
        let cursor_x = layout.x_for_index(cursor_offset);
        let current_scroll = self.scroll_handle.offset();
        let visible_width = bounds.size.width - px(CURSOR_WIDTH);
        let text_width = layout.width;
        let visible_left = current_scroll.x;
        let visible_right = current_scroll.x + visible_width;

        let mut new_scroll_x = current_scroll.x;

        if cursor_x < visible_left {
            new_scroll_x = cursor_x.max(px(0.0));
        } else if cursor_x >= visible_right {
            new_scroll_x = cursor_x - visible_width;
        }

        // Never leave blank space past the end of the text.
        if text_width <= visible_width {
            new_scroll_x = px(0.0);
        } else {
            let max_scroll = text_width - visible_width;
            new_scroll_x = new_scroll_x.min(max_scroll).max(px(0.0));
        }

        if new_scroll_x != current_scroll.x {
            let new_offset = point(new_scroll_x, current_scroll.y);
            self.scroll_handle.set_offset(new_offset);
        }
    }

    pub(super) fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    pub(super) fn display_cursor_offset(&self) -> usize {
        self.actual_to_display_offset(self.cursor_offset())
    }

    /// Offset into the displayed text, which differs from the value while masked.
    pub(super) fn actual_to_display_offset(&self, actual_offset: usize) -> usize {
        if !self.masked {
            return actual_offset;
        }

        self.value[..actual_offset].graphemes(true).count() * self.mask.len()
    }

    fn display_to_actual_offset(&self, display_offset: usize) -> usize {
        if !self.masked || self.mask.is_empty() {
            return display_offset;
        }

        TextOps::grapheme_offset_to_byte_offset(&self.value, display_offset / self.mask.len())
    }

    /// Window x coordinate at which `actual_offset` is painted.
    pub(super) fn x_for_offset(
        &self,
        line: &ShapedLine,
        bounds: Bounds<Pixels>,
        actual_offset: usize,
    ) -> Pixels {
        bounds.left() + line.x_for_index(self.actual_to_display_offset(actual_offset))
            - self.scroll_handle.offset().x
    }

    fn line_x(&self, bounds: Bounds<Pixels>, window_x: Pixels) -> Pixels {
        window_x - bounds.left() + self.scroll_handle.offset().x
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.value.is_empty() {
            return 0;
        }

        let (Some(bounds), Some(line)) = (self.last_bounds, self.last_layout.as_ref()) else {
            return 0;
        };

        let display_index = line.closest_index_for_x(self.line_x(bounds, position.x));
        self.display_to_actual_offset(display_index)
    }

    fn prepare_replace_text(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        cx: &mut Context<Self>,
    ) -> Option<(String, String, Range<usize>)> {
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| TextOps::range_from_utf16(&self.value, range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());
        if range.is_empty() && new_text.is_empty() && self.marked_range.is_none() {
            return None;
        }

        let new_text = if let Some(max_length) = self.max_length
            && !new_text.is_empty()
            && !self.ignore_history
        {
            let before_len = self.value[..range.start].graphemes(true).count();
            let after_len = self.value[range.end..].graphemes(true).count();
            let new_len = new_text.graphemes(true).count();

            let current_len = before_len + after_len;

            if current_len + new_len > max_length {
                let available_space = max_length.saturating_sub(current_len);
                if available_space == 0 {
                    return None;
                }

                let byte_offset =
                    TextOps::grapheme_offset_to_byte_offset(new_text, available_space);
                &new_text[..byte_offset]
            } else {
                new_text
            }
        } else {
            new_text
        };

        self.pause_cursor_blink(cx);
        self.push_history(new_text, &range);

        let new_value = format!(
            "{}{}{}",
            &self.value[0..range.start],
            new_text,
            &self.value[range.end..]
        );

        Some((new_text.into(), new_value, range))
    }
}

impl EntityInputHandler for InputState {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = TextOps::range_from_utf16(&self.value, &range_utf16);
        actual_range.replace(TextOps::range_to_utf16(&self.value, &range));
        Some(self.value[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: TextOps::range_to_utf16(&self.value, &self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| TextOps::range_to_utf16(&self.value, range))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (new_text, new_value, range) =
            match self.prepare_replace_text(range_utf16, new_text, cx) {
                Some(result) => result,
                None => return,
            };

        let new_cursor_pos = range.start + new_text.len();
        self.value = new_value.into();
        self.selected_range = new_cursor_pos..new_cursor_pos;
        self.marked_range = None;
        self.should_auto_scroll = true;
        self.last_layout = None;
        self.last_bounds = None;

        cx.emit(InputEvent::Input);
        self.update_scroll_offset(None, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (new_text, new_value, range) =
            match self.prepare_replace_text(range_utf16, new_text, cx) {
                Some(result) => result,
                None => return,
            };

        self.value = new_value.into();

        if !new_text.is_empty() {
            self.marked_range = Some(range.start..range.start + new_text.len());
        } else {
            self.marked_range = None;
        }

        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|range_utf16| TextOps::range_from_utf16(&self.value, range_utf16))
            .map(|new_range| (new_range.start + range.start)..(new_range.end + range.start))
            .unwrap_or_else(|| {
                let new_pos = range.start + new_text.len();
                new_pos..new_pos
            });

        self.should_auto_scroll = true;
        cx.emit(InputEvent::Input);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let line = self.last_layout.as_ref()?;
        let range = TextOps::range_from_utf16(&self.value, &range_utf16);

        Some(Bounds::from_corners(
            point(self.x_for_offset(line, bounds, range.start), bounds.top()),
            point(self.x_for_offset(line, bounds, range.end), bounds.bottom()),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let bounds = self.last_bounds?;
        if !bounds.contains(&point) {
            return None;
        }
        let line = self.last_layout.as_ref()?;

        let display_index = line.index_for_x(self.line_x(bounds, point.x))?;
        let actual_offset = self.display_to_actual_offset(display_index);
        Some(TextOps::offset_to_utf16(&self.value, actual_offset))
    }
}

impl EventEmitter<InputEvent> for InputState {}

impl Focusable for InputState {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for InputState {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex_1()
            .overflow_x_hidden()
            .child(TextElement::new(cx.entity()))
    }
}
