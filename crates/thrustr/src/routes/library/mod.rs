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
    },
};
use artwork::ArtworkReady;
use domain::game::{GameId, SectionIndex};
use gpui::{
    AnyElement, AppContext, Context, Entity, FocusHandle, Image, InteractiveElement, IntoElement,
    ParentElement, Pixels, Render, Resource, ScrollStrategy, SharedString, Styled, Task,
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

const CARD_WIDTH: Pixels = px(220.);
const CARD_MIN_GAP: Pixels = px(8.);
const CARD_ASPECT_RATIO: f32 = 2. / 3.;
const CARD_PADDING_REM: f32 = 0.75;
const CARD_INNER_GAP_REM: f32 = 0.75;
const CARD_TEXT_SIZE_REM: f32 = 0.9;
const CARD_ICON_SIZE_REM: f32 = 1.5;
/// Fixed so a card's height does not depend on whether its title has loaded.
const CARD_TITLE_HEIGHT_REM: f32 = 1.25;
const CARD_ROW_GAP_REM: f32 = 1.5;

const GRID_PADDING_REM: f32 = 2. - CARD_PADDING_REM;

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
    /// Bumped on every refresh so an earlier, slower query cannot overwrite the
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
            focus_handle: cx.focus_handle().tab_stop(true),
            selected: None,
            was_focused: false,
            num_cols: Rc::new(Cell::new(1)),
            scrollbar: None,
            search_query: SharedString::default(),
            _search_debounce: None,
            _tasks: Vec::new(),
        };

        let task = cx.listen("games", |page, cx| {
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
        // be the sole owner here and make_mut mutates in place. If this fires,
        // something started holding the Rc across frames and make_mut is
        // silently cloning the whole cache.
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
            entry.cover_path = Some(path.clone());
            entry.accent_color = update.accent_color.map(accent_hsla);

            let resource = Resource::Path(path);
            self.image_cache
                .update(cx, |cache, cx| cache.remove(&resource, cx));
            cx.notify();
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

                        library.ids = Rc::new(index.ids);
                        library.sections = Rc::new(index.sections);
                        library.chunks = Rc::new(ChunkCache::new(MAX_RESIDENT_CHUNKS));
                        library.loading_chunks.clear();
                        library.generation += 1;

                        library.selected =
                            selected_id.and_then(|id| library.ids.iter().position(|&g| g == id));
                        library.restore_scroll(&old_ids, anchor);
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
        if let Some(next) = next {
            self.selected = Some(next);
            self.scroll_handle
                .scroll_to_item(next / cols, ScrollStrategy::Nearest);
            if let Some(scrollbar) = &self.scrollbar {
                scrollbar.update(cx, |scrollbar, cx| scrollbar.flash(cx));
            }
            cx.notify();
        }
    }

    /// Height of one grid row from the last layout.
    fn measured_row_height(&self, rows: usize) -> Option<Pixels> {
        if rows == 0 {
            return None;
        }
        self.scroll_handle
            .0
            .borrow()
            .last_item_size
            .map(|size| size.contents.height / rows as f32)
            .filter(|&h| h > Pixels::ZERO)
    }

    fn top_visible_item(&self) -> Option<usize> {
        let cols = self.num_cols.get().max(1);
        let count = self.ids.len();
        if count == 0 {
            return None;
        }

        let Some(row_height) = self.measured_row_height(count.div_ceil(cols)) else {
            // Not laid out yet, so nothing has scrolled.
            return Some(0);
        };

        let offset = self.scroll_handle.0.borrow().base_handle.offset().y.abs();
        let top_row = (offset / row_height).round() as usize;
        Some((top_row * cols).min(count - 1))
    }

    /// Whether the card's row is at least partially inside the viewport.
    fn is_item_visible(&self, idx: usize) -> bool {
        let cols = self.num_cols.get().max(1);
        let Some(row_height) = self.measured_row_height(self.ids.len().div_ceil(cols)) else {
            return false;
        };
        let list = self.scroll_handle.0.borrow();
        let viewport = list.base_handle.bounds().size.height;
        let offset = list.base_handle.offset().y.abs();

        let top = row_height * (idx / cols) as f32;
        top < offset + viewport && top + row_height > offset
    }

    /// Index anchoring the viewport across a games refresh.
    fn scroll_anchor(&self) -> Option<usize> {
        self.selected
            .filter(|&idx| self.is_item_visible(idx))
            .or_else(|| self.top_visible_item())
    }

    /// Scroll the refreshed list back to roughly the games that were on screen.
    fn restore_scroll(&mut self, old_ids: &[GameId], old_anchor: Option<usize>) {
        let Some(old_anchor) = old_anchor else { return };
        let Some(new_anchor) = old_ids[old_anchor..]
            .iter()
            .find_map(|old| self.ids.iter().position(|new| new == old))
        else {
            return;
        };

        let cols = self.num_cols.get().max(1);
        let old_row = old_anchor / cols;
        let new_row = new_anchor / cols;
        if new_row == old_row {
            return;
        }

        let Some(row_height) = self.measured_row_height(old_ids.len().div_ceil(cols)) else {
            return;
        };
        let mut offset = self.scroll_handle.0.borrow().base_handle.offset();

        offset.y -= row_height * (new_row as f32 - old_row as f32);
        self.scroll_handle.0.borrow().base_handle.set_offset(offset);
    }

    fn activate_selected(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.selected.and_then(|idx| self.ids.get(idx)).copied() {
            cx.navigate(Page::Game(id));
        }
    }

    fn ensure_window(&mut self, items: Range<usize>, cx: &mut Context<Self>) {
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
                    // `ids` was replaced while this hydration was in flight,
                    // its chunk index no longer refers to the same games.
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

struct GridDims {
    num_cols: usize,
    num_rows: usize,
    visible_rows: usize,
}

impl GridDims {
    fn compute(
        grid_width: Pixels,
        grid_height: Pixels,
        game_count: usize,
        content_height: Option<Pixels>,
    ) -> Self {
        let num_cols = ((grid_width + CARD_MIN_GAP) / (CARD_WIDTH + CARD_MIN_GAP)).floor() as usize;
        let num_cols = num_cols.max(1);
        let num_rows = game_count.div_ceil(num_cols);

        let visible_rows = content_height
            .filter(|h| *h > Pixels::ZERO)
            .zip(NonZeroUsize::new(num_rows))
            .map(|(content, rows)| {
                let row_height = content / rows.get() as f32;
                (grid_height / row_height).ceil() as usize
            })
            .unwrap_or(0);

        Self {
            num_cols,
            num_rows,
            visible_rows,
        }
    }

    fn cache_capacity(&self) -> usize {
        self.num_cols * (self.visible_rows + CACHE_OVERSCAN_ROWS)
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
                let padding = px(GRID_PADDING_REM * window.rem_size().as_f32());
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
                                        .update(cx, |library, cx| library.ensure_window(items, cx))
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
                                            .pb(rems(CARD_ROW_GAP_REM))
                                            .children((start..end).map(|idx| {
                                                chunks
                                                    .peek(&(idx / CHUNK_SIZE))
                                                    .and_then(|entries| {
                                                        entries.get(idx % CHUNK_SIZE)
                                                    })
                                                    .map(|game| GameCard::new(game.clone()))
                                                    .unwrap_or_else(GameCard::blank)
                                                    .selected(focused && selected == Some(idx))
                                            }))
                                            .children(
                                                (0..dims.num_cols - (end - start))
                                                    .map(|_| GameCard::filler()),
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
