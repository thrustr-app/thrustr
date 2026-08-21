use crate::{
    app::Route,
    conversions::image::image_to_gpui,
    extensions::{EventListenerExt, SpawnTaskExt},
    globals::{ArtworkServiceExt, ComponentRegistryExt, GameServiceExt},
    navigation::{NavigatorExt, Page},
    routes::library::{
        bubble::index_bubble,
        cache::{LruImageCache, lru_image_cache},
        card::{GameCard, GameEntry, accent_hsla, cover_path},
        grid::{GridDims, GridMetrics},
    },
};
use artwork::ArtworkReady;
use domain::game::{GameId, SectionIndex};
use event::Topic;
use gpui::{
    AnyElement, AppContext, Context, Entity, FocusHandle, Image, InteractiveElement, IntoElement,
    ParentElement, Pixels, Rems, Render, Resource, ScrollStrategy, SharedString, Styled, Task,
    UniformListScrollHandle, Window, container_query, div, px, rems, uniform_list,
};
use lru::LruCache;
use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    num::NonZeroUsize,
    ops::Range,
    rc::Rc,
    sync::Arc,
    time::Duration,
};
use theme::ThemeExt;
use tokio::sync::broadcast::error::RecvError;
use tracing::error;
use ui::{
    Activate, GRID_CONTEXT, GridDir, ListScrollbar, ScrollbarState, SelectDown, SelectLeft,
    SelectRight, SelectUp, WithVariant, grid_step, input, list_scrollbar_state,
};

mod bubble;
mod cache;
mod card;
mod grid;

const CARD_WIDTH: Pixels = px(220.);
const CARD_MIN_GAP: Pixels = px(8.);
const CARD_ASPECT_RATIO: f32 = 2. / 3.;
const CARD_PADDING: Rems = rems(0.75);
const CARD_INNER_GAP: Rems = rems(0.75);
const CARD_TEXT_SIZE: Rems = rems(0.9);
const CARD_ICON_SIZE: Rems = rems(1.5);
const CARD_TITLE_HEIGHT: Rems = rems(1.25);
const CARD_ROW_GAP: Rems = rems(1.5);

const GRID_PADDING: Rems = rems(2. - CARD_PADDING.0);

const CACHE_OVERSCAN_ROWS: usize = 3;

const CHUNK_SIZE: usize = 120;
const PREFETCH_CHUNKS: usize = 1;
/// Max hydrated chunks kept resident, with LRU eviction rather than distance-from-viewport.
/// `uniform_list` renders row 0 every frame for measuring item height and distance-based
/// eviction around that probe range would evict the chunks that are actually on screen.
const MAX_RESIDENT_CHUNKS: NonZeroUsize = NonZeroUsize::new(12).unwrap();

const SEARCH_DEBOUNCE: Duration = Duration::from_millis(50);

type ChunkCache = LruCache<usize, Vec<GameEntry>>;

pub struct Library {
    ids: Rc<Vec<GameId>>,
    sections: Rc<SectionIndex>,
    scroll_handle: UniformListScrollHandle,
    chunks: Rc<ChunkCache>,
    loading_chunks: HashSet<usize>,
    /// Bumped whenever `ids` is replaced so in-flight hydrations from a previous
    /// generation are discarded.
    generation: u64,
    /// Bumped on every refresh so an earlier query cannot overwrite the
    /// results of a later one when they resolve out of order.
    refresh_seq: u64,
    component_icons: HashMap<String, Arc<Image>>,
    image_cache: Entity<LruImageCache>,
    focus_handle: FocusHandle,
    selected: Option<usize>,
    was_focused: bool,
    num_cols: Rc<Cell<usize>>,
    scrollbar: Option<Entity<ScrollbarState>>,
    search_query: SharedString,
    _search_debounce: Option<Task<()>>,
    _tasks: Vec<Task<()>>,
}

impl Library {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut page = Self {
            ids: Rc::new(Vec::new()),
            sections: Rc::new(SectionIndex::default()),
            scroll_handle: UniformListScrollHandle::new(),
            chunks: Rc::new(ChunkCache::new(MAX_RESIDENT_CHUNKS)),
            loading_chunks: HashSet::new(),
            generation: 0,
            refresh_seq: 0,
            component_icons: HashMap::new(),
            image_cache: cx.new(|cx| LruImageCache::new(1, cx)),
            focus_handle: cx.focus_handle().tab_stop(false),
            selected: None,
            was_focused: false,
            num_cols: Rc::new(Cell::new(1)),
            scrollbar: None,
            search_query: SharedString::default(),
            _search_debounce: None,
            _tasks: Vec::new(),
        };

        let task = cx.listen(Topic::Games, |page, cx| {
            page.refresh_icons(cx);
            page.refresh_games(cx);
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

        page.refresh_icons(cx);
        page.refresh_games(cx);
        page
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
            entry.accent_color = update.accent_color.map(accent_hsla);

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

    fn refresh_icons(&mut self, cx: &mut Context<Self>) {
        self.component_icons = cx
            .storefronts()
            .iter()
            .filter_map(|s| {
                let meta = s.component().metadata();
                meta.icon
                    .map(|icon| (meta.id.to_string(), image_to_gpui(icon)))
            })
            .collect();
    }

    fn refresh_games(&mut self, cx: &mut Context<Self>) {
        let game_service = cx.game_service();

        self.refresh_seq += 1;
        let seq = self.refresh_seq;
        let query = self.search_query.clone();
        cx.spawn_and_update(
            async move { game_service.list_index(Some(&query)) },
            move |library, result, _| {
                if library.refresh_seq != seq {
                    return;
                }

                match result {
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
                        library.sections = Rc::new(index.sections);
                        library.chunks = Rc::new(ChunkCache::new(MAX_RESIDENT_CHUNKS));
                        library.loading_chunks.clear();
                        library.generation += 1;

                        library.selected = selected_id.and_then(|id| positions.get(&id).copied());
                        library.restore_scroll(&old_ids, anchor, &positions);
                    }
                    Err(e) => {
                        error!("failed to list game index: {e:#}");
                    }
                };
            },
        );
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
        let cols = self.num_cols.get().max(1);
        let next = match self.selected {
            None => self.top_visible_item(),
            Some(_) => grid_step(self.selected, dir, self.ids.len(), cols),
        };

        if let Some(next) = next.filter(|&next| Some(next) != self.selected) {
            self.selected = Some(next);
            self.scroll_handle
                .scroll_to_item(next / cols, ScrollStrategy::Nearest);
            if let Some(scrollbar) = &self.scrollbar {
                scrollbar.update(cx, |scrollbar, cx| scrollbar.flash(cx));
            }
            cx.notify();
        }
    }

    fn metrics(&self, count: usize) -> Option<GridMetrics> {
        let cols = self.num_cols.get().max(1);
        GridMetrics::measure(&self.scroll_handle, count.div_ceil(cols))
    }

    fn top_visible_item(&self) -> Option<usize> {
        let cols = self.num_cols.get().max(1);
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
        let cols = self.num_cols.get().max(1);
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

        let cols = self.num_cols.get().max(1);
        let old_row = old_anchor / cols;
        let new_row = new_anchor / cols;
        if new_row == old_row {
            return;
        }

        // The layout still describes the list being replaced.
        let Some(metrics) = self.metrics(old_ids.len()) else {
            return;
        };
        let mut offset = self.scroll_handle.0.borrow().base_handle.offset();

        offset.y -= metrics.scroll_delta(old_row, new_row);
        self.scroll_handle.0.borrow().base_handle.set_offset(offset);
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
            } else if !self.loading_chunks.contains(&chunk_idx) {
                self.hydrate_chunk(chunk_idx, cx);
            }
        }
    }

    fn hydrate_chunk(&mut self, chunk_idx: usize, cx: &mut Context<Self>) {
        let start = chunk_idx * CHUNK_SIZE;
        let end = (start + CHUNK_SIZE).min(self.ids.len());
        let ids: Vec<GameId> = self.ids[start..end].to_vec();
        let generation = self.generation;
        let game_service = cx.game_service();

        self.loading_chunks.insert(chunk_idx);
        cx.spawn_and_update(
            async move { game_service.list_by_ids(&ids) },
            move |library, result, _| {
                if library.generation != generation {
                    return;
                }
                match result {
                    Ok(items) => {
                        library.loading_chunks.remove(&chunk_idx);
                        let entries = items
                            .into_iter()
                            .map(|item| GameEntry::from_list_item(item, &library.component_icons))
                            .collect();
                        library.chunks_mut().push(chunk_idx, entries);
                    }
                    Err(e) => {
                        // Deliberately keep the in-flight marker, since dropping it
                        // would retry at frame rate against a database that is
                        // already failing. The next games refresh clears it.
                        error!(chunk_idx, "failed to hydrate games chunk: {e:#}");
                    }
                };
            },
        );
    }
}

impl Route for Library {
    fn header(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let library = cx.weak_entity();
        Some(
            input("library-search")
                .variant_outline()
                .placeholder("Search library")
                .value(self.search_query.clone())
                .leading_icon("icons/search.svg")
                .clear_button()
                .w(rems(28.))
                .rounded_full()
                .px(rems(1.2))
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

        let scrollbar = list_scrollbar_state("library-scrollbar", &self.scroll_handle, window, cx);
        self.scrollbar = Some(scrollbar.clone());
        let scroll_handle = self.scroll_handle.clone();
        let sections = self.sections.clone();

        let is_focused = self.focus_handle.is_focused(window);
        if is_focused && !self.was_focused && window.last_input_was_keyboard() {
            self.selected = self.top_visible_item();
            if let Some(idx) = self.selected {
                let cols = self.num_cols.get().max(1);
                self.scroll_handle
                    .scroll_to_item(idx / cols, ScrollStrategy::Nearest);
            }
        }
        self.was_focused = is_focused;

        // The bubble is placed from the offset this frame starts with, and
        // `uniform_list` only applies a queued `scroll_to_item` later in its
        // prepaint. Nothing else redraws after a keyboard move, so the bubble
        // would sit a frame behind the selection.
        let pending_scroll = {
            let list = self.scroll_handle.0.borrow();
            list.deferred_scroll_to_item
                .is_some()
                .then(|| list.base_handle.offset())
        };
        if let Some(offset) = pending_scroll {
            cx.on_next_frame(window, move |library, _, cx| {
                if library.scroll_handle.0.borrow().base_handle.offset() != offset {
                    cx.notify();
                }
            });
        }

        let focused = is_focused && window.last_input_was_keyboard();
        let selected = self.selected;
        let num_cols = self.num_cols.clone();

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
            .child(container_query(move |size, window, cx| {
                let padding = px(GRID_PADDING.0 * window.rem_size().as_f32());
                let content_height = {
                    let list = scroll_handle.0.borrow();
                    let viewport = list.base_handle.bounds().size.height;
                    let max_offset = list.base_handle.max_offset().y;
                    (viewport > Pixels::ZERO).then_some(max_offset + viewport)
                };
                let dims = GridDims::compute(
                    size.width - padding * 2.,
                    size.height,
                    game_count,
                    content_height,
                );
                num_cols.set(dims.num_cols);

                let bubble = index_bubble(&scrollbar, &scroll_handle, &sections, &dims, size, cx);

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
                                        let start = row_idx * dims.num_cols;
                                        let end = (start + dims.num_cols).min(game_count);

                                        div()
                                            .w_full()
                                            .flex()
                                            .justify_between()
                                            .px(padding)
                                            .pb(CARD_ROW_GAP)
                                            .children((start..end).map(|idx| {
                                                chunks
                                                    .peek(&(idx / CHUNK_SIZE))
                                                    .and_then(|entries| {
                                                        entries.get(idx % CHUNK_SIZE)
                                                    })
                                                    .map(|game| GameCard::new(game.clone()))
                                                    .unwrap_or_else(GameCard::unloaded)
                                                    .selected(focused && selected == Some(idx))
                                            }))
                                            .children(
                                                (0..dims.num_cols - (end - start))
                                                    .map(|_| GameCard::spacer()),
                                            )
                                    })
                                    .collect()
                            }
                        })
                        .track_scroll(&scroll_handle)
                        .with_decoration(ListScrollbar::new(scrollbar.clone()))
                        .size_full(),
                    )
                    .children(bubble)
            }))
    }
}
