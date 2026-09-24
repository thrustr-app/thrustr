use super::{ROUTE_PADDING, Route, cover_path};
use crate::{
    adapters::ColorExt,
    context::{EventListenerExt, SpawnTaskExt},
    globals::{ArtworkServiceExt, GameServiceExt},
    navigation::{NavigatorExt, Page},
};
use artwork::ArtworkReady;
use cache::{LruImageCache, lru_image_cache};
use card::{GameCard, GameEntry};
use domain::{game::GameId, section_index::SectionIndex};
use event::Topic;
use gpui::{
    AnyElement, App, AppContext, Context, Div, Entity, FocusHandle, InteractiveElement,
    IntoElement, ParentElement, Pixels, Point, Rems, Render, Resource, ScrollHandle,
    ScrollStrategy, SharedString, Styled, Subscription, Task, UniformListScrollHandle, Window,
    container_query, div, px, rems, uniform_list,
};
use grid::{GridDims, GridMetrics};
use lru::LruCache;
use std::{
    cell::Cell, collections::HashMap, num::NonZeroUsize, ops::Range, rc::Rc, time::Duration,
};
use theme::ThemeExt;
use tokio::sync::broadcast::error::RecvError;
use tracing::error;
use ui::{
    Activate, GRID_CONTEXT, GridDir, Icon, ListScrollbar, SCROLLBAR_WIDTH, ScrollbarState,
    Scrubber, SelectDown, SelectLeft, SelectRight, SelectUp, WithRadius, WithSize, grid_step,
    input, scrubber_position,
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

fn available_letters(sections: &SectionIndex) -> u32 {
    sections.sections().iter().fold(0, |mask, section| {
        scrubber_position(&section.label).map_or(mask, |i| mask | (1 << i))
    })
}

pub struct Library {
    ids: Rc<Vec<GameId>>,
    sections: Rc<SectionIndex>,
    available_letters: u32,

    pinned_letter: Option<usize>,
    pinned_offset: Option<Point<Pixels>>,
    scroll_handle: UniformListScrollHandle,
    selected: Option<usize>,
    num_cols: Rc<Cell<usize>>,
    scrollbar: Entity<ScrollbarState>,

    chunks: Rc<ChunkCache>,
    loading_chunks: HashMap<usize, Task<()>>,

    search_query: SharedString,
    _search_debounce: Option<Task<()>>,

    image_cache: Entity<LruImageCache>,

    focus_handle: FocusHandle,
    _focus_subscription: Subscription,
    _refresh_task: Option<Task<()>>,
    _tasks: Vec<Task<()>>,
}

impl Library {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle().tab_stop(false);
        let focus_subscription = cx.on_focus(&focus_handle, window, |this, window, cx| {
            if window.last_input_was_keyboard() {
                this.selected = this.top_visible_item();
                if let Some(idx) = this.selected {
                    let cols = this.cols();
                    this.scroll_handle
                        .scroll_to_item(idx / cols, ScrollStrategy::Nearest);
                }
                cx.defer_in(window, |_, _, cx| cx.notify());
            }
        });

        let scroll_handle = UniformListScrollHandle::new();
        let scrollbar = cx.new(|_| ScrollbarState::for_uniform_list(&scroll_handle));

        let mut page = Self {
            ids: Rc::new(Vec::new()),
            sections: Rc::new(SectionIndex::default()),
            available_letters: 0,
            pinned_letter: None,
            pinned_offset: None,
            scroll_handle,
            selected: None,
            num_cols: Rc::new(Cell::new(1)),
            scrollbar,
            chunks: Rc::new(ChunkCache::new(MAX_RESIDENT_CHUNKS)),
            loading_chunks: HashMap::new(),
            search_query: SharedString::default(),
            _search_debounce: None,
            image_cache: cx.new(|cx| LruImageCache::new(1, cx)),
            focus_handle,
            _focus_subscription: focus_subscription,
            _refresh_task: None,
            _tasks: Vec::new(),
        };

        let task = cx.listen(Topic::Games, Self::refresh_games);
        page._tasks.push(task);

        let task = cx.listen(Topic::Component, |_, cx| cx.notify());
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

        page.refresh_games(cx);
        page
    }

    fn cols(&self) -> usize {
        self.num_cols.get().max(1)
    }

    /// The list's inner scroll handle.
    fn base_scroll(&self) -> ScrollHandle {
        self.scroll_handle.0.borrow().base_handle.clone()
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

    fn refresh_games(&mut self, cx: &mut Context<Self>) {
        let game_service = cx.game_service();

        let query = self.search_query.clone();
        self._refresh_task = Some(cx.spawn_and_update(
            async move { game_service.list_index(Some(&query)) },
            |library, result, _| match result {
                Ok(index) => {
                    let selected_id = library
                        .selected
                        .and_then(|idx| library.ids.get(idx))
                        .copied();
                    let anchor = library.scroll_anchor();
                    let old_ids = library.ids.clone();

                    let positions: HashMap<GameId, usize> = index
                        .ids
                        .iter()
                        .enumerate()
                        .map(|(idx, &id)| (id, idx))
                        .collect();

                    library.focus_handle =
                        library.focus_handle.clone().tab_stop(!index.ids.is_empty());

                    library.ids = Rc::new(index.ids);
                    library.available_letters = available_letters(&index.sections);
                    library.sections = Rc::new(index.sections);
                    library.pinned_letter = None;
                    library.pinned_offset = None;
                    library.chunks = Rc::new(ChunkCache::new(MAX_RESIDENT_CHUNKS));
                    library.loading_chunks.clear();

                    library.selected = selected_id.and_then(|id| positions.get(&id).copied());
                    library.restore_scroll(&old_ids, anchor, &positions);
                }
                Err(e) => {
                    error!("failed to list game index: {e:#}");
                }
            },
        ));
    }

    fn set_query(&mut self, query: SharedString, cx: &mut Context<Self>) {
        if query == self.search_query {
            return;
        }
        self.search_query = query;

        self._search_debounce = Some(cx.spawn(async move |library, cx| {
            cx.background_executor().timer(SEARCH_DEBOUNCE).await;
            library
                .update(cx, |library, cx| library.refresh_games(cx))
                .ok();
        }));
    }

    fn move_selection(&mut self, dir: GridDir, cx: &mut Context<Self>) {
        let cols = self.cols();
        let next = match self.selected {
            None => self.top_visible_item(),
            Some(_) => grid_step(self.selected, dir, self.ids.len(), cols),
        };

        if let Some(next) = next.filter(|&next| Some(next) != self.selected) {
            self.selected = Some(next);
            self.scroll_handle
                .scroll_to_item(next / cols, ScrollStrategy::Nearest);
            self.scrollbar
                .update(cx, |scrollbar, cx| scrollbar.flash(cx));
            cx.notify();
        }
    }

    fn jump_to_section(&mut self, label: &str, cx: &mut Context<Self>) {
        let Some(start) = self.sections.start_of(label) else {
            return;
        };
        let cols = self.cols();
        self.scroll_handle
            .scroll_to_item(start / cols, ScrollStrategy::Top);
        self.pinned_letter = scrubber_position(label);
        self.pinned_offset = None;
        self.scrollbar
            .update(cx, |scrollbar, cx| scrollbar.flash(cx));
        cx.notify();
    }

    fn metrics(&self, count: usize) -> Option<GridMetrics> {
        let cols = self.cols();
        GridMetrics::measure(&self.scroll_handle, count.div_ceil(cols))
    }

    fn top_visible_item(&self) -> Option<usize> {
        let cols = self.cols();
        let count = self.ids.len();
        if count == 0 {
            return None;
        }

        let Some(metrics) = self.metrics(count) else {
            return Some(0);
        };
        Some((metrics.nearest_row() * cols).min(count - 1))
    }

    fn is_item_visible(&self, idx: usize) -> bool {
        let cols = self.cols();
        self.metrics(self.ids.len())
            .is_some_and(|metrics| metrics.row_is_visible(idx / cols))
    }

    /// Index anchoring the viewport across a games refresh.
    fn scroll_anchor(&self) -> Option<usize> {
        self.selected
            .filter(|&idx| self.is_item_visible(idx))
            .or_else(|| self.top_visible_item())
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

        let cols = self.cols();
        let old_row = old_anchor / cols;
        let new_row = new_anchor / cols;
        if new_row == old_row {
            return;
        }

        // The layout still describes the list being replaced.
        let Some(metrics) = self.metrics(old_ids.len()) else {
            return;
        };
        let base = self.base_scroll();
        let mut offset = base.offset();

        offset.y -= metrics.scroll_delta(old_row, new_row);
        base.set_offset(offset);
    }

    fn activate_selected(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.selected.and_then(|idx| self.ids.get(idx)).copied() {
            cx.navigate(Page::Game(id));
        }
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

    fn track_pinned_letter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let pending_scroll = {
            let list = self.scroll_handle.0.borrow();
            list.deferred_scroll_to_item
                .is_some()
                .then(|| list.base_handle.offset())
        };
        if let Some(offset) = pending_scroll {
            cx.on_next_frame(window, move |library, _, cx| {
                if library.base_scroll().offset() != offset {
                    cx.notify();
                }
            });
        }

        if self.pinned_letter.is_some() && pending_scroll.is_none() {
            let live_offset = self.base_scroll().offset();
            match self.pinned_offset {
                None => self.pinned_offset = Some(live_offset),
                Some(offset) if offset != live_offset => {
                    self.pinned_letter = None;
                    self.pinned_offset = None;
                }
                _ => {}
            }
        }
    }
}

fn render_row(
    row_idx: usize,
    dims: &GridDims,
    chunks: &ChunkCache,
    game_count: usize,
    padding: Pixels,
    focused: bool,
    selected: Option<usize>,
) -> Div {
    let start = row_idx * dims.num_cols;
    let end = (start + dims.num_cols).min(game_count);

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
                .map(|game| GameCard::new(game.clone(), dims.card_width))
                .unwrap_or_else(|| GameCard::unloaded(dims.card_width))
                .selected(focused && selected == Some(idx))
        }))
        .children((0..dims.num_cols - (end - start)).map(|_| GameCard::spacer(dims.card_width)))
}

impl Route for Library {
    const PADDING: Rems = rems(0.);

    fn header(&self, this: &Entity<Self>, _cx: &App) -> Option<AnyElement> {
        let library = this.downgrade();
        Some(
            input("library-search")
                .placeholder("Search library")
                .value(self.search_query.clone())
                .leading_icon(Icon::search())
                .radius_pill()
                .size_lg()
                .clear_button()
                .w(rems(28.))
                .on_input(move |event, _, cx| {
                    let query = event.value.clone();
                    library
                        .update(cx, |library, cx| library.set_query(query, cx))
                        .ok();
                })
                .into_any_element(),
        )
    }
}

impl Render for Library {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let game_count = self.ids.len();
        let chunks = self.chunks.clone();
        let image_cache = self.image_cache.clone();
        let library = cx.weak_entity();

        let scrollbar = self.scrollbar.clone();
        let scroll_handle = self.scroll_handle.clone();
        let base_scroll = self.base_scroll();
        let sections = self.sections.clone();

        let is_focused = self.focus_handle.is_focused(window);
        self.track_pinned_letter(window, cx);

        let focused = is_focused && window.last_input_was_keyboard();
        let selected = self.selected;
        let num_cols = self.num_cols.clone();
        let available_letters = self.available_letters;
        let pinned_letter = self.pinned_letter;

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
                let viewport = base_scroll.bounds().size.height;
                let content_height =
                    (viewport > Pixels::ZERO).then(|| base_scroll.max_offset().y + viewport);
                let dims = GridDims::compute(
                    size.width - padding * 2.,
                    size.height,
                    game_count,
                    content_height,
                );
                num_cols.set(dims.num_cols);

                let current_letter = pinned_letter.or_else(|| {
                    GridMetrics::measure(&scroll_handle, dims.num_rows)
                        .and_then(|metrics| {
                            sections.label_for(metrics.first_touching_row() * dims.num_cols)
                        })
                        .and_then(scrubber_position)
                });

                let scrubber = (!sections.is_empty()).then(|| {
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
                    .image_cache(lru_image_cache(image_cache.clone(), dims.cache_capacity()))
                    .child(
                        uniform_list("game-grid", dims.num_rows, {
                            let chunks = chunks.clone();
                            let library = library.clone();
                            move |range, _, cx| {
                                let items = range.start * dims.num_cols
                                    ..(range.end * dims.num_cols).min(game_count);
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
                                            row_idx, &dims, &chunks, game_count, padding, focused,
                                            selected,
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
