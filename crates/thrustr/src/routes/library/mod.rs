use crate::{
    conversions::image::image_to_gpui,
    extensions::{EventListenerExt, SpawnTaskExt},
    globals::{ArtworkServiceExt, ComponentRegistryExt, GameServiceExt},
    navigation::{NavigatorExt, Page},
    routes::library::{
        bubble::index_bubble,
        cache::{LruImageCache, lru_image_cache},
    },
};
use artwork::ArtworkReady;
use config::paths;
use domain::{
    artwork::Color,
    game::{GameId, GameListItem, SectionIndex},
};
use gpui::{
    App, AppContext, Context, Entity, FontWeight, Hsla, Image, ImageSource, InteractiveElement,
    IntoElement, ObjectFit, ParentElement, Pixels, Render, RenderOnce, Resource, SharedString,
    StatefulInteractiveElement, Styled, StyledImage, Task, UniformListScrollHandle, Window,
    container_query, div, img, px, rems, rgb, uniform_list,
};
use lru::LruCache;
use std::{
    collections::{HashMap, HashSet},
    num::NonZeroUsize,
    ops::Range,
    path::Path,
    rc::Rc,
    sync::Arc,
};
use theme::ThemeExt;
use tokio::sync::broadcast::error::RecvError;
use tracing::error;
use ui::{ListScrollbar, list_scrollbar_state};

mod bubble;
mod cache;

const GAME_CARD_WIDTH: Pixels = px(220.);
const MIN_GAP: Pixels = px(8.);
const CARD_ASPECT_RATIO: f32 = 2. / 3.;
const CARD_PADDING_REM: f32 = 0.75;
const CARD_INNER_GAP_REM: f32 = 0.75;
const CARD_TEXT_SIZE_REM: f32 = 0.9;
const CARD_ICON_SIZE_REM: f32 = 1.5;
/// Fixed so a card's height does not depend on whether its title has loaded.
const CARD_TITLE_HEIGHT_REM: f32 = 1.25;
const GRID_PADDING_REM: f32 = 2. - CARD_PADDING_REM;

const CHUNK_SIZE: usize = 120;
const PREFETCH_CHUNKS: usize = 1;
/// Max hydrated chunks kept resident, with LRU eviction rather than distance-from-viewport.
/// `uniform_list` also invokes the render closure with a probe range (row 0) every frame
/// to measure item height, and distance-based eviction around that probe range
/// would evict the chunks that are actually on screen.
const MAX_RESIDENT_CHUNKS: NonZeroUsize = NonZeroUsize::new(12).unwrap();

type ChunkCache = LruCache<usize, Vec<GameEntry>>;

#[derive(Clone)]
struct GameEntry {
    id: GameId,
    element_id: SharedString,
    name: SharedString,
    cover_url: Option<SharedString>,
    cover_path: Option<Arc<Path>>,
    accent_color: Option<Hsla>,
    source_icon: Option<Arc<Image>>,
}

impl GameEntry {
    fn from_list_item(item: GameListItem, icons: &HashMap<String, Arc<Image>>) -> Self {
        let (cover_path, accent_color) = match item.cover {
            Some(art) => (
                Some(cover_path(&art.hash)),
                art.accent_color.map(accent_hsla),
            ),
            None => (None, None),
        };
        Self {
            id: item.id,
            element_id: item.id.to_string().into(),
            name: item.name.into(),
            cover_url: item.cover_url.map(Into::into),
            source_icon: icons.get(&item.source_id).cloned(),
            cover_path,
            accent_color,
        }
    }
}

fn cover_path(hash: &str) -> Arc<Path> {
    paths::artwork_path(hash, "webp").into()
}

fn accent_hsla(color: Color) -> Hsla {
    rgb(color.into()).into()
}

#[derive(IntoElement)]
struct GameCard {
    game: Option<GameEntry>,
    filler: bool,
}

impl GameCard {
    fn new(game: GameEntry) -> Self {
        Self {
            game: Some(game),
            filler: false,
        }
    }

    fn blank() -> Self {
        Self {
            game: None,
            filler: false,
        }
    }

    fn filler() -> Self {
        Self {
            game: None,
            filler: true,
        }
    }
}

impl RenderOnce for GameCard {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();

        if self.filler {
            return div().flex_shrink_0().w(GAME_CARD_WIDTH).into_any_element();
        }

        let base = div()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap(rems(CARD_INNER_GAP_REM))
            .p(rems(CARD_PADDING_REM))
            .w(GAME_CARD_WIDTH)
            .rounded(theme.radius.lg)
            .bg(theme.colors.card_background.opacity(0.));

        let mut cover = div()
            .aspect_ratio(CARD_ASPECT_RATIO)
            .w_full()
            .bg(theme.colors.card_background)
            .rounded(theme.radius.md);

        let mut title = div()
            .h(rems(CARD_TITLE_HEIGHT_REM))
            .overflow_hidden()
            .whitespace_nowrap()
            .w_full()
            .text_ellipsis()
            .text_color(theme.colors.primary)
            .text_size(rems(CARD_TEXT_SIZE_REM))
            .font_weight(FontWeight::LIGHT);

        let mut icon_row = div().h(rems(CARD_ICON_SIZE_REM)).flex_shrink_0();

        let Some(game) = self.game else {
            return base
                .child(cover)
                .child(title)
                .child(icon_row)
                .into_any_element();
        };

        if let Some(path) = game.cover_path {
            let mut cover_img = img(ImageSource::Resource(Resource::Path(path)))
                .object_fit(ObjectFit::Contain)
                .w_full()
                .h_full()
                .rounded(theme.radius.md);

            if let Some(url) = game.cover_url {
                let artwork_service = cx.artwork_service();
                cover_img = cover_img.with_fallback(move || {
                    artwork_service.enqueue_cover(game.id, &url);
                    div().into_any_element()
                });
            }

            cover = cover.child(cover_img);
        }

        title = title.child(game.name);

        if let Some(icon) = game.source_icon {
            icon_row = icon_row.child(img(ImageSource::Image(icon)).size(rems(CARD_ICON_SIZE_REM)));
        }

        base.id(game.element_id.clone())
            .on_click(move |_, _, cx| {
                cx.navigate(Page::Game(game.id));
            })
            .hover(|style| {
                style.bg(game
                    .accent_color
                    .unwrap_or(theme.colors.card_background)
                    .opacity(0.25))
            })
            .child(cover)
            .child(title)
            .child(icon_row)
            .into_any_element()
    }
}

pub struct Library {
    ids: Rc<Vec<GameId>>,
    sections: Rc<SectionIndex>,
    scroll_handle: UniformListScrollHandle,
    chunks: Rc<ChunkCache>,
    loading_chunks: HashSet<usize>,
    /// Bumped whenever `ids` is replaced so in-flight hydrations from a previous
    /// generation are discarded.
    generation: u64,
    component_icons: HashMap<String, Arc<Image>>,
    image_cache: Entity<LruImageCache>,
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
            component_icons: HashMap::new(),
            image_cache: cx.new(|cx| LruImageCache::new(1, cx)),
            _tasks: Vec::new(),
        };

        let task = cx.listen("games", |page, cx| page.refresh_games(cx));
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

    fn refresh_games(&mut self, cx: &mut Context<Self>) {
        let game_service = cx.game_service();

        self.component_icons = cx
            .storefronts()
            .iter()
            .filter_map(|s| {
                let meta = s.component().metadata();
                meta.icon
                    .map(|icon| (meta.id.to_string(), image_to_gpui(icon)))
            })
            .collect();

        cx.spawn_and_update(
            async move { game_service.list_index() },
            |library, result, _| {
                match result {
                    Ok(index) => {
                        library.ids = Rc::new(index.ids);
                        library.sections = Rc::new(index.sections);
                        library.chunks = Rc::new(ChunkCache::new(MAX_RESIDENT_CHUNKS));
                        library.loading_chunks.clear();
                        library.generation += 1;
                    }
                    Err(e) => {
                        error!("failed to list game index: {e:#}");
                    }
                };
            },
        );
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
    fn compute(grid_width: Pixels, grid_height: Pixels, game_count: usize, rem_size: f32) -> Self {
        let num_cols = ((grid_width + MIN_GAP) / (GAME_CARD_WIDTH + MIN_GAP)).floor() as usize;
        let num_cols = num_cols.max(1);
        let num_rows = game_count.div_ceil(num_cols);

        let visible_rows =
            (grid_height.as_f32() / Self::card_height_px(rem_size)).ceil() as usize + 2;

        Self {
            num_cols,
            num_rows,
            visible_rows,
        }
    }

    /// Approximate full card height in pixels.
    fn card_height_px(rem_size: f32) -> f32 {
        let image_height = GAME_CARD_WIDTH.as_f32() / CARD_ASPECT_RATIO;
        let chrome = (CARD_PADDING_REM * 2.
            + CARD_INNER_GAP_REM * 2.
            + CARD_TEXT_SIZE_REM
            + CARD_ICON_SIZE_REM)
            * rem_size;
        image_height + chrome
    }

    fn cache_capacity(&self) -> usize {
        self.num_cols * self.visible_rows
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
        let scroll_handle = self.scroll_handle.clone();
        let sections = self.sections.clone();

        div()
            .flex_grow_1()
            .text_color(theme.colors.accent)
            .child(container_query(move |size, window, cx| {
                let padding = px(GRID_PADDING_REM * window.rem_size().as_f32());
                let dims = GridDims::compute(
                    size.width - padding * 2.,
                    size.height,
                    game_count,
                    window.rem_size().as_f32(),
                );

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
                                            .pb(rems(1.5))
                                            .children((start..end).map(|idx| {
                                                chunks
                                                    .peek(&(idx / CHUNK_SIZE))
                                                    .and_then(|entries| {
                                                        entries.get(idx % CHUNK_SIZE)
                                                    })
                                                    .map(|game| GameCard::new(game.clone()))
                                                    .unwrap_or_else(GameCard::blank)
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
