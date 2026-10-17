use crate::{
    ScrollbarState,
    components::{
        input::{Backspace, InputEvent, InputState},
        menu::{Cancel, Confirm, SelectNext, SelectNextPage, SelectPrev, SelectPrevPage, Step},
    },
};
use gpui::{
    App, AppContext, Bounds, Context, Entity, EventEmitter, FocusHandle, Focusable, MouseDownEvent,
    Pixels, ScrollStrategy, SharedString, Subscription, UniformListScrollHandle, Window,
};
use std::mem;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComboboxEvent {
    Change,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelectionMode {
    #[default]
    Single,
    Multiple,
}

pub struct ComboboxState {
    pub(super) input: Entity<InputState>,
    pub(super) list: UniformListScrollHandle,
    pub(super) scrollbar: Entity<ScrollbarState>,
    pub(super) bounds: Bounds<Pixels>,
    pub(super) items: Vec<SharedString>,
    /// Lowercased `items`.
    keys: Vec<String>,
    pub(super) values: Vec<SharedString>,
    selected: Vec<usize>,
    placeholder: SharedString,
    /// Lowercased search text.
    query: String,
    /// Whether the input holds text typed by the user.
    filtering: bool,
    input_dirty: bool,
    pub(super) matches: Vec<usize>,
    pub(super) is_open: bool,
    pub(super) highlighted: Option<usize>,
    /// Row count the menu list was last laid out with.
    laid_out_count: usize,
    /// Height of a menu row, measured by the last list layout.
    row_height: Option<Pixels>,
    mode: SelectionMode,
    _subscriptions: [Subscription; 2],
}

impl EventEmitter<ComboboxEvent> for ComboboxState {}

impl ComboboxState {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::new_with_mode(SelectionMode::Single, window, cx)
    }

    pub fn multiple(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::new_with_mode(SelectionMode::Multiple, window, cx)
    }

    pub fn new_with_mode(mode: SelectionMode, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx));
        let focus_handle = input.focus_handle(cx);

        let _subscriptions = [
            cx.subscribe(&input, |this, input, event, cx| {
                if *event == InputEvent::Input {
                    let text = input.read(cx).value().clone();
                    this.search(&text, cx);
                }
            }),
            cx.on_blur(&focus_handle, window, |_, window, cx| {
                cx.defer_in(window, |this, _, cx| this.close(cx));
            }),
        ];

        let list = UniformListScrollHandle::new();
        let scrollbar = cx.new(|_| ScrollbarState::for_uniform_list(&list));

        Self {
            input,
            list,
            scrollbar,
            bounds: Bounds::default(),
            items: Vec::new(),
            keys: Vec::new(),
            values: Vec::new(),
            selected: Vec::new(),
            placeholder: SharedString::default(),
            query: String::new(),
            filtering: false,
            input_dirty: false,
            matches: Vec::new(),
            is_open: false,
            highlighted: None,
            laid_out_count: 0,
            row_height: None,
            mode,
            _subscriptions,
        }
    }

    pub fn mode(&self) -> SelectionMode {
        self.mode
    }

    pub fn value(&self) -> Option<&SharedString> {
        self.values.first()
    }

    pub fn values(&self) -> &[SharedString] {
        &self.values
    }

    pub fn with_value(self, value: impl Into<SharedString>) -> Self {
        self.with_values([value])
    }

    /// Only the first value is kept in single mode.
    pub fn with_values(
        mut self,
        values: impl IntoIterator<Item = impl Into<SharedString>>,
    ) -> Self {
        self.replace_values(values.into_iter().map(Into::into));
        self
    }

    /// Replace the value without emitting a [`ComboboxEvent`].
    pub fn set_value(&mut self, value: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.set_values([value], cx);
    }

    /// Replace the values without emitting a [`ComboboxEvent`]. Only the first
    /// value is kept in single mode.
    pub fn set_values(
        &mut self,
        values: impl IntoIterator<Item = impl Into<SharedString>>,
        cx: &mut Context<Self>,
    ) {
        self.replace_values(values.into_iter().map(Into::into));
        cx.notify();
    }

    /// Clear the values without emitting a [`ComboboxEvent`].
    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.replace_values([]);
        cx.notify();
    }

    pub fn items(&self) -> &[SharedString] {
        &self.items
    }

    pub fn with_items(mut self, items: impl IntoIterator<Item = impl Into<SharedString>>) -> Self {
        self.replace_items(items);
        self
    }

    pub fn set_items(
        &mut self,
        items: impl IntoIterator<Item = impl Into<SharedString>>,
        cx: &mut Context<Self>,
    ) {
        self.replace_items(items);
        cx.notify();
    }

    pub fn with_placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self.input_dirty = true;
        self
    }

    pub fn set_placeholder(
        &mut self,
        placeholder: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) {
        self.placeholder = placeholder.into();
        self.input_dirty = true;
        cx.notify();
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.focus_handle(cx).focus(window, cx);
    }

    pub(super) fn is_selected(&self, item_ix: usize) -> bool {
        self.selected.contains(&item_ix)
    }

    fn replace_items(&mut self, items: impl IntoIterator<Item = impl Into<SharedString>>) {
        self.items.clear();
        self.items.extend(items.into_iter().map(Into::into));
        self.keys.clear();
        self.keys
            .extend(self.items.iter().map(|item| item.to_lowercase()));
        self.refilter();
        self.highlighted = self
            .highlighted
            .filter(|_| !self.matches.is_empty())
            .map(|ix| ix.min(self.matches.len() - 1));
        self.reselect();
    }

    fn replace_values(&mut self, values: impl IntoIterator<Item = SharedString>) {
        self.values.clear();
        self.values.extend(values);
        self.values_changed();
    }

    fn values_changed(&mut self) {
        if !self.is_multiple() {
            self.values.truncate(1);
        }
        self.reselect();
        self.input_dirty = true;
    }

    pub(super) fn is_multiple(&self) -> bool {
        self.mode == SelectionMode::Multiple
    }

    fn reselect(&mut self) {
        self.selected.clear();
        self.selected.extend(
            self.values
                .iter()
                .filter_map(|value| self.items.iter().position(|item| item == value)),
        );
    }

    pub(super) fn prepare(&mut self, interactive: bool, cx: &mut Context<Self>) {
        self.sync_input(cx);
        self.measure_row_height(self.is_open && interactive);
    }

    fn sync_input(&mut self, cx: &mut Context<Self>) {
        if !mem::take(&mut self.input_dirty) {
            return;
        }
        let text = (!self.filtering).then(|| match self.is_multiple() {
            true => SharedString::default(),
            false => self.values.first().cloned().unwrap_or_default(),
        });
        let placeholder = match self.is_multiple() && !self.values.is_empty() {
            true => SharedString::default(),
            false => self.placeholder.clone(),
        };
        self.input.update(cx, |input, _| {
            if let Some(text) = text {
                input.replace_value(text);
            }
            input.replace_placeholder(placeholder);
        });
    }

    fn measure_row_height(&mut self, menu_visible: bool) {
        if let Some(size) = self.list.0.borrow().last_item_size
            && self.laid_out_count > 0
            && size.contents.height > Pixels::ZERO
        {
            self.row_height = Some(size.contents.height / self.laid_out_count as f32);
        }
        if menu_visible && !self.matches.is_empty() {
            self.laid_out_count = self.matches.len();
        }
    }

    fn refilter(&mut self) {
        self.matches.clear();
        if self.filtering && !self.query.is_empty() {
            let query = self.query.as_str();
            self.matches.extend(
                self.keys
                    .iter()
                    .enumerate()
                    .filter(|(_, key)| key.contains(query))
                    .map(|(ix, _)| ix),
            );
        } else {
            self.matches.extend(0..self.items.len());
        }
    }

    fn search(&mut self, text: &str, cx: &mut Context<Self>) {
        if text.is_empty() && !self.is_multiple() && !self.values.is_empty() {
            self.replace_values([]);
            cx.emit(ComboboxEvent::Change);
        }
        self.filtering = true;
        self.query.clear();
        self.query.push_str(&text.trim().to_lowercase());
        self.refilter();
        self.is_open = true;
        self.highlighted = (!self.matches.is_empty()).then_some(0);
        self.list.scroll_to_item(0, ScrollStrategy::Top);
        cx.notify();
    }

    fn stop_filtering(&mut self) {
        if self.filtering {
            self.filtering = false;
            self.input_dirty = true;
            self.query.clear();
            self.refilter();
        }
    }

    pub(super) fn open(&mut self, cx: &mut Context<Self>) {
        if self.is_open {
            return;
        }
        self.is_open = true;
        self.stop_filtering();

        let initial = match self.is_multiple() {
            true => None,
            false => self.selected.first().copied(),
        };
        self.highlighted = initial.or((!self.matches.is_empty()).then_some(0));
        if let Some(ix) = self.highlighted {
            self.list.scroll_to_item(ix, ScrollStrategy::Center);
        }
        cx.notify();
    }

    pub(super) fn close(&mut self, cx: &mut Context<Self>) {
        if !self.is_open && !self.filtering {
            return;
        }
        self.is_open = false;
        self.highlighted = None;
        self.stop_filtering();
        cx.notify();
    }

    pub(super) fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let focus_handle = self.focus_handle(cx);
        if !focus_handle.is_focused(window) {
            focus_handle.focus(window, cx);
        }
        match self.is_open {
            true => self.close(cx),
            false => self.open(cx),
        }
    }

    pub(super) fn highlight(&mut self, ix: usize, cx: &mut Context<Self>) {
        if self.highlighted != Some(ix) {
            self.highlighted = Some(ix);
            cx.notify();
        }
    }

    fn reveal(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.highlight(ix, cx);
        self.list.scroll_to_item(ix, ScrollStrategy::Nearest);
    }

    pub(super) fn list_overflows(&self, max_height: Pixels) -> bool {
        self.row_height
            .is_some_and(|height| height * self.matches.len() as f32 > max_height)
    }

    fn page_size(&self) -> usize {
        let viewport = self
            .list
            .0
            .borrow()
            .last_item_size
            .map(|size| size.item.height);
        viewport
            .zip(self.row_height)
            .map_or(1, |(viewport, row)| (viewport / row) as usize)
            .max(1)
    }

    fn step(&mut self, step: Step, cx: &mut Context<Self>) {
        if !self.is_open {
            self.open(cx);
            return;
        }
        let count = self.matches.len();
        if count == 0 {
            return;
        }
        let target = step.target(self.highlighted, count, self.page_size());
        self.reveal(target, cx);
    }

    pub(super) fn choose(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(&item_ix) = self.matches.get(ix) else {
            return;
        };
        let item = self.items[item_ix].clone();

        if self.is_multiple() {
            match self.values.iter().position(|value| *value == item) {
                Some(pos) => {
                    self.values.remove(pos);
                }
                None => self.values.push(item),
            }
            self.values_changed();
            cx.emit(ComboboxEvent::Change);
            self.stop_filtering();
            // Unfiltered now so the row is the item index.
            self.reveal(item_ix, cx);
        } else {
            if self.values.first() != Some(&item) {
                self.replace_values([item]);
                cx.emit(ComboboxEvent::Change);
            }
            self.close(cx);
        }
        cx.notify();
    }

    pub(super) fn remove(&mut self, pos: usize, cx: &mut Context<Self>) {
        if pos < self.values.len() {
            self.values.remove(pos);
            self.values_changed();
            cx.emit(ComboboxEvent::Change);
            cx.notify();
        }
    }

    pub(super) fn dismiss(
        &mut self,
        event: &MouseDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.bounds.contains(&event.position) {
            self.close(cx);
        }
    }

    pub(super) fn set_bounds(&mut self, bounds: Bounds<Pixels>, cx: &mut Context<Self>) {
        let resized = bounds.size.width != self.bounds.size.width;
        self.bounds = bounds;
        if resized && self.is_open {
            cx.notify();
        }
    }

    pub(super) fn select_prev(&mut self, _: &SelectPrev, _: &mut Window, cx: &mut Context<Self>) {
        self.step(Step::Prev, cx);
    }

    pub(super) fn select_next(&mut self, _: &SelectNext, _: &mut Window, cx: &mut Context<Self>) {
        self.step(Step::Next, cx);
    }

    pub(super) fn select_prev_page(
        &mut self,
        _: &SelectPrevPage,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step(Step::PrevPage, cx);
    }

    pub(super) fn select_next_page(
        &mut self,
        _: &SelectNextPage,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step(Step::NextPage, cx);
    }

    pub(super) fn confirm(&mut self, _: &Confirm, _: &mut Window, cx: &mut Context<Self>) {
        match (self.is_open, self.highlighted) {
            (true, Some(ix)) => self.choose(ix, cx),
            (true, None) => {}
            (false, _) => self.open(cx),
        }
    }

    pub(super) fn cancel(&mut self, _: &Cancel, _: &mut Window, cx: &mut Context<Self>) {
        self.close(cx);
    }

    pub(super) fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_multiple() && !self.values.is_empty() && self.input.read(cx).value().is_empty() {
            cx.stop_propagation();
            self.remove(self.values.len() - 1, cx);
        }
    }
}

impl Focusable for ComboboxState {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.focus_handle(cx)
    }
}
