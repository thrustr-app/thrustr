use super::{ROUTE_PADDING, Route, cover_path};
use crate::{
    adapters::ColorExt,
    context::{EventListenerExt, SpawnTaskExt},
    globals::{ArtworkServiceExt, ComponentRegistryExt, GameServiceExt},
    navigation::{NavigatorExt, Page},
};
use artwork::ArtworkReady;
use cache::{LruImageCache, lru_image_cache};
use card::{GameCard, GameEntry};
use domain::{
    component::StorefrontOperation,
    game::{GameId, GameIndex},
    section_index::SectionIndex,
};
use event::Topic;
use gpui::{
    AnyElement, App, AppContext, Context, Div, Entity, FocusHandle, InteractiveElement,
    IntoElement, ParentElement, Pixels, Rems, Render, Resource, ScrollHandle, SharedString, Styled,
    Subscription, Task, UniformListScrollHandle, Window, container_query, div,
    prelude::FluentBuilder, px, rems, uniform_list,
};
use grid::{GridLayout, GridViewport};
use lru::LruCache;
use std::{
    cell::Cell, collections::HashMap, num::NonZeroUsize, ops::Range, rc::Rc, time::Duration,
};
use theme::ThemeExt;
use tokio::sync::broadcast::error::RecvError;
use tracing::error;
use ui::{
    Activate, Button, GRID_CONTEXT, GridDir, Icon, Input, InputEvent, InputState, ListScrollbar,
    SCROLLBAR_WIDTH, ScrollbarState, Scrubber, SelectDown, SelectLeft, SelectRight, SelectUp,
    WithRadius, WithSize, WithVariant, grid_step, scrubber_position,
};

mod cache;
mod card;
mod grid;

const CARD_MIN_WIDTH: Pixels = px(180.);
const CARD_GAP: Pixels = px(8.);
const CARD_ASPECT_RATIO: f32 = 2. / 3.;
const CARD_PADDING: Rems = rems(0.5);
const CARD_INNER_GAP: Rems = rems(0.625);
const CARD_TITLE_SIZE: Rems = rems(0.875);
const CARD_ICON_SIZE: Rems = rems(1.25);
const CARD_ROW_GAP: Rems = rems(1.25);

const GRID_PADDING: Rems = rems(ROUTE_PADDING.0 - CARD_PADDING.0);
// FIXME: this should be 4px but because gpui does not have built-in support for outlines,
// the game card has to set a 1px border. An extra px here makes it visually more centered.
const SCRUBBER_GAP: Pixels = px(5.);

const CACHE_OVERSCAN_ROWS: usize = 3;

const CHUNK_SIZE: usize = 120;
const PREFETCH_CHUNKS: usize = 1;
/// Max hydrated chunks kept resident, with LRU eviction rather than distance-from-viewport.
/// `uniform_list` renders row 0 every frame for measuring item height and distance-based
/// eviction around that probe range would evict the chunks that are actually on screen.
const MAX_RESIDENT_CHUNKS: NonZeroUsize = NonZeroUsize::new(12).unwrap();

const SEARCH_DEBOUNCE: Duration = Duration::from_millis(50);

type ChunkCache = LruCache<usize, Vec<GameEntry>>;

#[derive(Clone, Default)]
pub struct LibraryState {
    search_query: SharedString,
    anchor: Option<ViewAnchor>,
    selected: Option<GameId>,
}

#[derive(Clone, Copy)]
struct ViewAnchor {
    game: GameId,
    rows_from_top: f32,
}

#[derive(Clone, Copy)]
struct PinnedLetter {
    letter: usize,
    offset: Pixels,
}

pub struct Library {
    ids: Rc<Vec<GameId>>,
    sections: Rc<SectionIndex>,
    available_letters: u32,
    chunks: Rc<ChunkCache>,
    loading_chunks: HashMap<usize, Task<()>>,
    image_cache: Entity<LruImageCache>,
    selected: Option<usize>,
    pending_selection: Option<GameId>,
    focus_handle: FocusHandle,
    scroll_handle: UniformListScrollHandle,
    last_layout: Rc<Cell<Option<GridLayout>>>,
    scrollbar: Entity<ScrollbarState>,
    pending_anchor: Option<ViewAnchor>,
    pinned: Option<PinnedLetter>,
    search: Entity<InputState>,
    _search_debounce: Option<Task<()>>,
    syncing: bool,
    can_sync: bool,
    _focus_subscription: Subscription,
    _search_subscription: Subscription,
    _refresh_task: Option<Task<()>>,
    _tasks: Vec<Task<()>>,
}

impl Route for Library {
    const PADDING: Rems = rems(0.);

    type Args = ();
    type State = LibraryState;

    fn build(_: (), state: LibraryState, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle().tab_stop(false);
        let focus_subscription = cx.on_focus(&focus_handle, window, |this, window, cx| {
            if window.last_input_was_keyboard() {
                if !this.is_restoring() {
                    this.selected = this.anchor_item();
                    if let Some(idx) = this.selected {
                        this.reveal_item(idx);
                    }
                }
                cx.defer_in(window, |_, _, cx| cx.notify());
            }
        });

        let search = cx.new(|cx| {
            InputState::new(window, cx)
                .with_value(state.search_query.clone())
                .with_placeholder("Search library")
        });
        let search_subscription = cx.subscribe(&search, |this, _, event, cx| {
            if *event == InputEvent::Input {
                this.restart_search(cx);
            }
        });

        let scroll_handle = UniformListScrollHandle::new();
        let scrollbar = cx.new(|_| ScrollbarState::for_uniform_list(&scroll_handle));

        let mut page = Self {
            ids: Rc::new(Vec::new()),
            sections: Rc::new(SectionIndex::default()),
            available_letters: 0,
            chunks: Rc::new(ChunkCache::new(MAX_RESIDENT_CHUNKS)),
            loading_chunks: HashMap::new(),
            image_cache: cx.new(|cx| LruImageCache::new(1, cx)),
            selected: None,
            pending_selection: state.selected,
            focus_handle,
            scroll_handle,
            last_layout: Rc::new(Cell::new(None)),
            scrollbar,
            pending_anchor: state.anchor,
            pinned: None,
            search,
            _search_debounce: None,
            syncing: false,
            can_sync: false,
            _focus_subscription: focus_subscription,
            _search_subscription: search_subscription,
            _refresh_task: None,
            _tasks: Vec::new(),
        };

        let task = cx.listen(Topic::Games, Self::refresh_games);
        page._tasks.push(task);

        // To refresh the component icons used by game cards.
        let task = cx.listen(Topic::ComponentRegistered, |this, cx| {
            this.refresh_sync_state(cx);
            cx.notify();
        });
        page._tasks.push(task);

        let task = cx.listen(Topic::ComponentState, |this, cx| {
            if this.refresh_sync_state(cx) {
                cx.notify();
            }
        });
        page._tasks.push(task);

        let mut artwork_rx = cx.artwork_service().subscribe();
        let artwork_task = cx.spawn(async move |library, cx| {
            loop {
                match artwork_rx.recv().await {
                    Ok(update) => {
                        library
                            .update(cx, |lib, cx| lib.apply_artwork_update(update, cx))
                            .ok();
                    }
                    Err(RecvError::Lagged(_)) => {}
                    Err(RecvError::Closed) => break,
                }
            }
        });
        page._tasks.push(artwork_task);

        page.refresh_sync_state(cx);
        page.refresh_games(cx);

        if page.is_restoring() {
            page.focus_handle.focus(window, cx);
        }
        page
    }

    fn header(&self, _: &Entity<Self>, _cx: &App) -> Option<AnyElement> {
        Some(
            div()
                .flex()
                .items_center()
                .gap(rems(0.5))
                .child(
                    Input::new(&self.search)
                        .leading_icon(Icon::search())
                        .radius_pill()
                        .size_lg()
                        .clear_button()
                        .w(rems(28.)),
                )
                .child(
                    Button::icon(
                        "sync-storefronts",
                        Icon::refresh().when(self.syncing, Icon::spin),
                    )
                    .variant_field()
                    .radius_pill()
                    .size_lg()
                    .when(!self.can_sync, Button::disabled)
                    .on_click(|_, _, cx| {
                        let registry = cx.component_registry();
                        cx.background_spawn(async move { registry.sync_storefronts().await })
                            .detach();
                    }),
                )
                .into_any_element(),
        )
    }

    fn save_state(&self, next: &Page, cx: &App) -> Option<LibraryState> {
        let opened = match next {
            Page::Game(id) => Some(*id),
            _ => None,
        };

        Some(LibraryState {
            search_query: self.search.read(cx).value().clone(),
            anchor: self.pending_anchor.or_else(|| self.view_anchor(opened)),
            selected: opened
                .or_else(|| self.selected_id())
                .or(self.pending_selection),
        })
    }
}

impl Library {
    /// Returns whether the sync state changed.
    fn refresh_sync_state(&mut self, cx: &App) -> bool {
        let storefronts = cx.storefronts();
        let syncing = storefronts
            .iter()
            .any(|s| s.component().is_running(StorefrontOperation::Sync));
        let can_sync = storefronts
            .iter()
            .any(|s| s.component().can(StorefrontOperation::Sync));

        let changed = (syncing, can_sync) != (self.syncing, self.can_sync);
        self.syncing = syncing;
        self.can_sync = can_sync;
        changed
    }

    fn refresh_games(&mut self, cx: &mut Context<Self>) {
        let game_service = cx.game_service();

        let query = self.search.read(cx).value().clone();
        self._refresh_task = Some(cx.spawn_and_update(
            async move { game_service.list_index(Some(&query)) },
            |library, result, _| match result {
                Ok(index) => library.apply_game_index(index),
                Err(e) => error!("failed to list game index: {e:#}"),
            },
        ));
    }

    fn restart_search(&mut self, cx: &mut Context<Self>) {
        self._search_debounce = Some(cx.spawn(async move |library, cx| {
            cx.background_executor().timer(SEARCH_DEBOUNCE).await;
            library
                .update(cx, |library, cx| library.refresh_games(cx))
                .ok();
        }));
    }

    fn apply_game_index(&mut self, index: GameIndex) {
        let selected_id = self.selected_id().or(self.pending_selection.take());
        let anchor = self.anchor_item();
        let old_ids = self.ids.clone();

        let positions: HashMap<GameId, usize> = index
            .ids
            .iter()
            .enumerate()
            .map(|(idx, &id)| (id, idx))
            .collect();

        self.focus_handle = self.focus_handle.clone().tab_stop(!index.ids.is_empty());

        self.ids = Rc::new(index.ids);
        self.available_letters = available_letters(&index.sections);
        self.sections = Rc::new(index.sections);
        self.pinned = None;
        self.chunks = Rc::new(ChunkCache::new(MAX_RESIDENT_CHUNKS));
        self.loading_chunks.clear();

        self.selected = selected_id.and_then(|id| positions.get(&id).copied());
        self.restore_scroll(&old_ids, anchor, &positions);
        // Lets `save_state` fall back to the current view if the game is gone.
        self.pending_anchor = self
            .pending_anchor
            .filter(|anchor| positions.contains_key(&anchor.game));
    }

    fn move_selection(&mut self, dir: GridDir, cx: &mut Context<Self>) {
        let cols = self.num_cols();
        let next = match self.selected {
            None => self.top_visible_item(),
            Some(_) => grid_step(self.selected, dir, self.ids.len(), cols),
        };

        if let Some(next) = next.filter(|&next| Some(next) != self.selected) {
            self.selected = Some(next);
            self.reveal_item(next);
            self.scrollbar
                .update(cx, |scrollbar, cx| scrollbar.flash(cx));
            cx.notify();
        }
    }

    fn jump_to_section(&mut self, label: &str, cx: &mut Context<Self>) {
        let Some(start) = self.sections.start_of(label) else {
            return;
        };
        self.scroll_item_to_top(start);
        self.pinned = scrubber_position(label).map(|letter| PinnedLetter {
            letter,
            offset: self.scroll_offset(),
        });
        self.scrollbar
            .update(cx, |scrollbar, cx| scrollbar.flash(cx));
        cx.notify();
    }

    fn activate_selected(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.selected_id() {
            cx.navigate(Page::Game(id));
        }
    }

    fn selected_id(&self) -> Option<GameId> {
        self.selected.and_then(|idx| self.ids.get(idx)).copied()
    }

    fn num_cols(&self) -> usize {
        self.last_layout.get().map_or(1, |layout| layout.num_cols)
    }

    fn row_of(&self, idx: usize) -> usize {
        idx / self.num_cols()
    }

    fn viewport(&self) -> Option<GridViewport> {
        GridViewport::measure(&self.scroll_handle, self.last_layout.get()?.num_rows)
    }

    /// The list's inner scroll handle.
    fn base_scroll(&self) -> ScrollHandle {
        self.scroll_handle.0.borrow().base_handle.clone()
    }

    /// How far down the list is scrolled, as a positive distance.
    fn scroll_offset(&self) -> Pixels {
        -self.base_scroll().offset().y
    }

    fn set_scroll_offset(&self, offset: Pixels) {
        let base = self.base_scroll();
        let mut position = base.offset();
        position.y = -offset;
        base.set_offset(position);
    }

    fn scroll_item_to_top(&self, idx: usize) {
        if let Some(viewport) = self.viewport() {
            self.set_scroll_offset(viewport.offset_to_place(self.row_of(idx), 0.));
        }
    }

    fn reveal_item(&self, idx: usize) {
        if let Some(viewport) = self.viewport() {
            self.set_scroll_offset(viewport.offset_to_reveal(self.row_of(idx)));
        }
    }

    fn top_visible_item(&self) -> Option<usize> {
        let last = self.ids.len().checked_sub(1)?;
        let row = self
            .viewport()
            .map_or(0, |viewport| viewport.row_nearest_top());
        Some((row * self.num_cols()).min(last))
    }

    fn is_item_visible(&self, idx: usize) -> bool {
        self.viewport()
            .is_some_and(|viewport| viewport.is_row_visible(self.row_of(idx)))
    }

    /// Index anchoring the viewport across a games refresh.
    fn anchor_item(&self) -> Option<usize> {
        self.selected
            .filter(|&idx| self.is_item_visible(idx))
            .or_else(|| self.top_visible_item())
    }

    fn unpin_letter_if_scrolled(&mut self) {
        let offset = self.scroll_offset();
        self.pinned = self
            .pinned
            .filter(|pin| (pin.offset - offset).abs() < px(1.));
    }

    fn top_letter(&self) -> Option<usize> {
        let top_item = self.viewport()?.first_visible_row() * self.num_cols();
        self.sections
            .label_for(top_item)
            .and_then(scrubber_position)
    }

    fn is_restoring(&self) -> bool {
        self.pending_anchor.is_some() || self.pending_selection.is_some()
    }

    /// Scroll the refreshed list back to roughly the games that were on screen.
    fn restore_scroll(
        &mut self,
        old_ids: &[GameId],
        old_anchor: Option<usize>,
        positions: &HashMap<GameId, usize>,
    ) {
        let Some(old_anchor) = old_anchor else { return };
        let Some(&new_anchor) = old_ids[old_anchor..]
            .iter()
            .find_map(|old| positions.get(old))
        else {
            return;
        };

        let old_row = self.row_of(old_anchor);
        let new_row = self.row_of(new_anchor);
        if new_row == old_row {
            return;
        }

        let Some(viewport) = self.viewport() else {
            return;
        };
        self.set_scroll_offset(self.scroll_offset() + viewport.scroll_delta(old_row, new_row));
    }

    /// Anchors the viewport on the game being opened, or on the top visible one otherwise.
    fn view_anchor(&self, opened: Option<GameId>) -> Option<ViewAnchor> {
        let viewport = self.viewport()?;
        let idx = opened
            .and_then(|opened| self.ids.iter().position(|&id| id == opened))
            .or_else(|| self.top_visible_item())?;

        Some(ViewAnchor {
            game: self.ids[idx],
            rows_from_top: viewport.rows_from_top(self.row_of(idx)),
        })
    }

    fn apply_pending_anchor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(anchor) = self.pending_anchor else {
            return;
        };
        let Some(viewport) = self.viewport() else {
            if !self.ids.is_empty() {
                cx.on_next_frame(window, |_, _, cx| cx.notify());
            }
            return;
        };
        self.pending_anchor = None;

        let Some(idx) = self.ids.iter().position(|&id| id == anchor.game) else {
            return;
        };
        self.set_scroll_offset(viewport.offset_to_place(self.row_of(idx), anchor.rows_from_top));
    }

    fn ensure_chunks_resident(&mut self, items: Range<usize>, cx: &mut Context<Self>) {
        if self.ids.is_empty() || items.is_empty() {
            return;
        }

        let first = items.start / CHUNK_SIZE;
        let last = (items.end - 1) / CHUNK_SIZE;
        let max_chunk = (self.ids.len() - 1) / CHUNK_SIZE;
        let needed =
            first.saturating_sub(PREFETCH_CHUNKS)..=(last + PREFETCH_CHUNKS).min(max_chunk);

        for chunk_idx in needed {
            if self.chunks.contains(&chunk_idx) {
                self.chunks_mut().promote(&chunk_idx);
            } else if !self.loading_chunks.contains_key(&chunk_idx) {
                self.hydrate_chunk(chunk_idx, cx);
            }
        }
    }

    fn hydrate_chunk(&mut self, chunk_idx: usize, cx: &mut Context<Self>) {
        let start = chunk_idx * CHUNK_SIZE;
        let end = (start + CHUNK_SIZE).min(self.ids.len());
        let ids: Vec<GameId> = self.ids[start..end].to_vec();
        let game_service = cx.game_service();

        let task = cx.spawn_and_update(
            async move { game_service.list_by_ids(&ids) },
            move |library, result, _| match result {
                Ok(items) => {
                    library.loading_chunks.remove(&chunk_idx);
                    let entries = items.into_iter().map(GameEntry::from_list_item).collect();
                    library.chunks_mut().push(chunk_idx, entries);
                }
                Err(e) => {
                    // Keep the in-flight marker since dropping it would retry at frame rate
                    // against a database that is failing. The next games refresh clears it.
                    error!(chunk_idx, "failed to hydrate games chunk: {e:#}");
                }
            },
        );
        self.loading_chunks.insert(chunk_idx, task);
    }

    fn chunks_mut(&mut self) -> &mut ChunkCache {
        // Render clones this Rc into frame closures, but gpui drops the element
        // arena right after each draw and this runs between frames, so we should
        // be the sole owner here.
        debug_assert_eq!(
            Rc::strong_count(&self.chunks),
            1,
            "render frame still holds Rc clone - make_mut will clone the entire chunk cache"
        );
        Rc::make_mut(&mut self.chunks)
    }

    fn apply_artwork_update(&mut self, update: ArtworkReady, cx: &mut Context<Self>) {
        let position = self.chunks.iter().find_map(|(&chunk_idx, entries)| {
            entries
                .iter()
                .position(|g| g.id == update.game_id)
                .map(|offset| (chunk_idx, offset))
        });
        if let Some((chunk_idx, offset)) = position {
            let entry = &mut self.chunks_mut().peek_mut(&chunk_idx).unwrap()[offset];
            let path = cover_path(&update.hash);
            entry.cover_path = path.clone();
            entry.accent = update.accent.map(|c| c.to_gpui_hsla());

            if let Some(path) = path {
                let resource = Resource::Path(path);
                self.image_cache
                    .update(cx, |cache, cx| cache.remove(&resource, cx));
            }

            if self.is_item_visible(chunk_idx * CHUNK_SIZE + offset) {
                cx.notify();
            }
        }
    }
}

fn available_letters(sections: &SectionIndex) -> u32 {
    sections.sections().iter().fold(0, |mask, section| {
        scrubber_position(&section.label).map_or(mask, |i| mask | (1 << i))
    })
}

fn render_row(
    row_idx: usize,
    layout: &GridLayout,
    chunks: &ChunkCache,
    game_count: usize,
    padding: Pixels,
    focused: bool,
    selected: Option<usize>,
) -> Div {
    let start = row_idx * layout.num_cols;
    let end = (start + layout.num_cols).min(game_count);

    div()
        .w_full()
        .flex()
        .gap(CARD_GAP)
        .px(padding)
        .pb(CARD_ROW_GAP)
        .children((start..end).map(|idx| {
            chunks
                .peek(&(idx / CHUNK_SIZE))
                .and_then(|entries| entries.get(idx % CHUNK_SIZE))
                .map(|game| GameCard::new(game.clone(), layout.card_width))
                .unwrap_or_else(|| GameCard::unloaded(layout.card_width))
                .selected(focused && selected == Some(idx))
        }))
        .children((0..layout.num_cols - (end - start)).map(|_| GameCard::spacer(layout.card_width)))
}

impl Render for Library {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.apply_pending_anchor(window, cx);
        self.unpin_letter_if_scrolled();

        let theme = cx.theme();
        let game_count = self.ids.len();
        let chunks = self.chunks.clone();
        let image_cache = self.image_cache.clone();
        let library = cx.weak_entity();

        let scrollbar = self.scrollbar.clone();
        let scroll_handle = self.scroll_handle.clone();
        let last_layout = self.last_layout.clone();
        let has_sections = !self.sections.is_empty();

        let focused = self.focus_handle.is_focused(window) && window.last_input_was_keyboard();
        let selected = self.selected;
        let available_letters = self.available_letters;
        let current_letter = self
            .pinned
            .map(|pin| pin.letter)
            .or_else(|| self.top_letter());
        let visible_rows = self
            .viewport()
            .map_or(0, |viewport| viewport.visible_rows());

        div()
            .track_focus(&self.focus_handle)
            .key_context(GRID_CONTEXT)
            .on_action(
                cx.listener(|this, _: &SelectLeft, _, cx| this.move_selection(GridDir::Left, cx)),
            )
            .on_action(
                cx.listener(|this, _: &SelectRight, _, cx| this.move_selection(GridDir::Right, cx)),
            )
            .on_action(
                cx.listener(|this, _: &SelectUp, _, cx| this.move_selection(GridDir::Up, cx)),
            )
            .on_action(
                cx.listener(|this, _: &SelectDown, _, cx| this.move_selection(GridDir::Down, cx)),
            )
            .on_action(cx.listener(|this, _: &Activate, _, cx| this.activate_selected(cx)))
            .flex_grow_1()
            .text_color(theme.colors.accent)
            .child(container_query(move |size, window, _cx| {
                let padding = px(GRID_PADDING.0 * window.rem_size().as_f32());
                let layout = GridLayout::compute(size.width - padding * 2., game_count);
                last_layout.set(Some(layout));

                let scrubber = has_sections.then(|| {
                    let library = library.clone();
                    Scrubber::new()
                        .available(available_letters)
                        .current(current_letter)
                        .on_select(move |label, _, cx| {
                            library
                                .update(cx, |library, cx| library.jump_to_section(label, cx))
                                .ok();
                        })
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .right(SCROLLBAR_WIDTH + SCRUBBER_GAP)
                });

                div()
                    .size_full()
                    .relative()
                    .image_cache(lru_image_cache(
                        image_cache.clone(),
                        layout.cache_capacity(visible_rows),
                    ))
                    .child(
                        uniform_list("game-grid", layout.num_rows, {
                            let chunks = chunks.clone();
                            let library = library.clone();
                            move |range, _, cx| {
                                let items = range.start * layout.num_cols
                                    ..(range.end * layout.num_cols).min(game_count);
                                let library = library.clone();
                                cx.defer(move |cx| {
                                    library
                                        .update(cx, |library, cx| {
                                            library.ensure_chunks_resident(items, cx)
                                        })
                                        .ok();
                                });

                                range
                                    .map(|row_idx| {
                                        render_row(
                                            row_idx, &layout, &chunks, game_count, padding,
                                            focused, selected,
                                        )
                                    })
                                    .collect()
                            }
                        })
                        .track_scroll(&scroll_handle)
                        .with_decoration(ListScrollbar::new(scrollbar.clone()))
                        .size_full(),
                    )
                    .children(scrubber)
            }))
    }
}
