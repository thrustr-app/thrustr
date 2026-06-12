use crate::{
    conversions::image::image_to_gpui,
    extensions::{EventListenerExt, SpawnTaskExt},
    globals::{ArtworkServiceExt, ComponentRegistryExt, GameServiceExt},
    routes::library::cache::lru_image_cache,
};
use artwork::ArtworkReady;
use config::paths;
use domain::{artwork::Color, game::GameListItem};
use gpui::{
    App, Bounds, Context, FontWeight, Hsla, Image, ImageSource, InteractiveElement, IntoElement,
    ObjectFit, ParentElement, Pixels, Render, RenderOnce, Resource, SharedString, Styled,
    StyledImage, Task, Window, div, img, px, rems, rgb, uniform_list,
};
use std::{collections::HashMap, path::Path, rc::Rc, sync::Arc};
use theme::ThemeExt;
use tokio::sync::broadcast::error::RecvError;

mod cache;

const GAME_CARD_WIDTH: Pixels = px(220.);
const MIN_GAP: Pixels = px(8.);

const CARD_ASPECT_RATIO: f32 = 2. / 3.;
const CARD_PADDING_REM: f32 = 0.75;
const CARD_INNER_GAP_REM: f32 = 0.75;
const CARD_TEXT_SIZE_REM: f32 = 0.9;
const CARD_ICON_SIZE_REM: f32 = 1.5;

#[derive(Clone)]
struct GameEntry {
    id: SharedString,
    name: SharedString,
    cover_path: Option<Arc<Path>>,
    accent_color: Option<Hsla>,
    source_icon: Option<Arc<Image>>,
}

impl GameEntry {
    fn from_list_item(item: GameListItem, icons: &HashMap<String, Arc<Image>>) -> Self {
        let (cover_path, accent_color) = match item.artwork {
            Some(art) => (
                Some(cover_path(&art.hash)),
                art.accent_color.map(accent_hsla),
            ),
            None => (None, None),
        };
        Self {
            id: item.id.to_string().into(),
            name: item.name.into(),
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
    rgb(color.to_hex()).into()
}

#[derive(IntoElement)]
struct GameCard {
    game: Option<GameEntry>,
}

impl GameCard {
    fn new(game: GameEntry) -> Self {
        Self { game: Some(game) }
    }

    fn blank() -> Self {
        Self { game: None }
    }
}

impl RenderOnce for GameCard {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let group_name = self.game.as_ref().map(|g| g.id.clone()).unwrap_or_default();

        let base = div()
            .id(group_name.clone())
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap(rems(CARD_INNER_GAP_REM))
            .p(rems(CARD_PADDING_REM))
            .w(GAME_CARD_WIDTH)
            .group(group_name)
            .rounded(theme.radius.lg)
            .bg(theme.colors.card_background.opacity(0.));

        let Some(game) = self.game else {
            return base;
        };

        let mut cover = div()
            .aspect_ratio(2. / 3.)
            .w_full()
            .bg(theme.colors.card_background)
            .rounded(theme.radius.md);

        if let Some(path) = game.cover_path {
            cover = cover.child(
                img(ImageSource::Resource(Resource::Path(path)))
                    .object_fit(ObjectFit::Contain)
                    .w_full()
                    .h_full()
                    .rounded(theme.radius.md),
            );
        }

        let title = div()
            .overflow_hidden()
            .whitespace_nowrap()
            .w_full()
            .text_ellipsis()
            .text_color(theme.colors.primary)
            .text_size(rems(CARD_TEXT_SIZE_REM))
            .font_weight(FontWeight::LIGHT)
            .child(game.name);

        let icon_row = if let Some(icon) = game.source_icon {
            img(ImageSource::Image(icon))
                .size(rems(CARD_ICON_SIZE_REM))
                .into_any_element()
        } else {
            div().mb(rems(CARD_ICON_SIZE_REM)).into_any_element()
        };

        base.hover(|style| {
            style.bg(game
                .accent_color
                .unwrap_or(theme.colors.card_background)
                .opacity(0.25))
        })
        .child(cover)
        .child(title)
        .child(icon_row)
    }
}

pub struct Library {
    games: Rc<Vec<GameEntry>>,
    component_icons: HashMap<String, Arc<Image>>,
    grid_bounds: Bounds<Pixels>,
    _tasks: Vec<Task<()>>,
}

impl Library {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut page = Self {
            games: Rc::new(Vec::new()),
            grid_bounds: Bounds::default(),
            component_icons: HashMap::new(),
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

    fn apply_artwork_update(&mut self, update: ArtworkReady, cx: &mut Context<Self>) {
        let id_str = update.game_id.to_string();
        debug_assert_eq!(
            Rc::strong_count(&self.games),
            1,
            "render frame still holds Rc clone - make_mut will clone the entire games vec"
        );
        let games = Rc::make_mut(&mut self.games);
        if let Some(entry) = games.iter_mut().find(|g| g.id.as_ref() == id_str) {
            entry.cover_path = Some(cover_path(&update.hash));
            entry.accent_color = update.accent_color.map(accent_hsla);
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
            // TODO: implement actual pagination
            async move { game_service.list(0, 999999) },
            |library, result, _| {
                match result {
                    Ok(games) => {
                        library.games = Rc::new(
                            games
                                .into_iter()
                                .map(|item| {
                                    GameEntry::from_list_item(item, &library.component_icons)
                                })
                                .collect(),
                        );
                    }
                    Err(e) => {
                        println!("{:?}", e);
                    }
                };
            },
        );
    }

    fn set_bounds(&mut self, bounds: Vec<Bounds<Pixels>>, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(&new_bounds) = bounds.first()
            && self.grid_bounds != new_bounds
        {
            self.grid_bounds = new_bounds;
            let entity_id = cx.entity_id();
            cx.defer(move |cx| cx.notify(entity_id));
        }
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
        game_count: usize,
        window_height: Pixels,
        rem_size: f32,
    ) -> Self {
        let num_cols = ((grid_width + MIN_GAP) / (GAME_CARD_WIDTH + MIN_GAP)).floor() as usize;
        let num_cols = num_cols.max(1);
        let num_rows = (game_count as f32 / num_cols as f32).ceil() as usize;

        let visible_rows =
            (window_height.as_f32() / Self::card_height_px(rem_size)).ceil() as usize + 2;

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
        let rem_size = window.rem_size().as_f32();

        let dims = GridDims::compute(
            self.grid_bounds.size.width,
            self.games.len(),
            window.bounds().size.height,
            rem_size,
        );

        let games = self.games.clone();

        div()
            .on_children_prepainted(cx.processor(Self::set_bounds))
            .flex_grow_1()
            .px(rems(2. - CARD_PADDING_REM))
            .text_color(theme.colors.accent)
            .image_cache(lru_image_cache("game-grid-cache", dims.cache_capacity()))
            .child(
                uniform_list("game-grid", dims.num_rows, move |range, _, _| {
                    range
                        .map(|row_idx| {
                            let start = row_idx * dims.num_cols;
                            let end = (start + dims.num_cols).min(games.len());
                            let row = &games[start..end];
                            let blanks = dims.num_cols - row.len();

                            div()
                                .w_full()
                                .flex()
                                .justify_between()
                                .pb(rems(1.5))
                                .children(row.iter().map(|game| GameCard::new(game.clone())))
                                .children((0..blanks).map(|_| GameCard::blank()))
                        })
                        .collect()
                })
                .size_full(),
            )
    }
}
