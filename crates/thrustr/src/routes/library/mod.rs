use crate::{
    cache::lru_image_cache,
    conversions::image::image_to_gpui,
    extensions::{EventListenerExt, SpawnTaskExt},
    globals::{ComponentRegistryExt, GameServiceExt},
};
use config::paths;
use gpui::{
    App, Bounds, Context, FontWeight, Image, ImageSource, InteractiveElement, IntoElement, ObjectFit, ParentElement, Pixels, Render, RenderOnce, Resource, SharedString, Styled, StyledImage, Task, Window, div, img, prelude::FluentBuilder, px, rems, uniform_list
};
use std::{collections::HashMap, path::Path, rc::Rc, sync::Arc};
use theme::ThemeExt;

const GAME_CARD_WIDTH: Pixels = px(220.);
const MIN_GAP: Pixels = px(8.);

#[derive(Clone)]
struct GameEntry {
    id: SharedString,
    name: SharedString,
    cover_path: Arc<Path>,
    source_icon: Option<Arc<Image>>,
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

        let content = div()
            .id(group_name.clone())
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap(rems(0.75))
            .w(GAME_CARD_WIDTH)
            .group(group_name.clone())
            .p(rems(0.75))
            .rounded(theme.radius.lg)
            .bg(theme.colors.card_background.opacity(0.));

        match self.game {
            None => content,
            Some(game) => content
                .hover(|style| style.bg(theme.colors.card_background))
                .child(
                    div()
                        .aspect_ratio(2. / 3.)
                        .w_full()
                        .bg(theme.colors.card_background)
                        .rounded(theme.radius.md)
                        .child(
                            img(ImageSource::Resource(Resource::Path(game.cover_path)))
                                .object_fit(ObjectFit::Contain)
                                .w_full()
                                .h_full()
                                .rounded(theme.radius.md),
                        ),
                )
                .child(
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .w_full()
                        .text_ellipsis()
                        .child(game.name)
                        .text_color(theme.colors.primary)
                        .text_size(rems(0.9))
                        .font_weight(FontWeight::LIGHT),
                )
                .when_else(game.source_icon.is_some(), |this| {
                    this.child(img(ImageSource::Image(game.source_icon.unwrap())).size(rems(1.5)))
                }, |this| {
                    this.mb(rems(1.5))
                }),
        }
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

        let task = cx.listen("games", |page, cx| {
            page.refresh_games(cx);
        });
        page._tasks.push(task);

        page.refresh_games(cx);
        page
    }

    fn refresh_games(&mut self, cx: &mut Context<Self>) {
        let game_service = cx.game_service();

        let component_icons: HashMap<String, Arc<Image>> = cx
            .storefronts()
            .iter()
            .filter_map(|s| {
                let meta = s.component().metadata();
                meta.icon.map(|icon| {
                    (meta.id.to_string(), image_to_gpui(icon))
                })
            })
            .collect();

        self.component_icons = component_icons;

        cx.spawn_and_update(
            async move { game_service.list(0, 999999) },
            |library, result, _| {
                match result {
                    Ok(games) => {
                        library.games = Rc::new(games.into_iter().map(|g| GameEntry {
                            id: g.id.to_string().into(),
                            source_icon: library.component_icons.get(&g.source_id).cloned(),
                            name: g.name.into(),
                            cover_path: paths::cover_path(g.id.into(), "webp").into(),
                        }).collect());
                    }
                    Err(e) => {
                        println!("{:?}", e);
                    }
                };
            },
        );
    }

    fn set_bounds(&mut self, bounds: Vec<Bounds<Pixels>>, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(bounds) = bounds.first()
            && self.grid_bounds != *bounds
        {
            self.grid_bounds = *bounds;
            let entity_id = cx.entity_id();
            cx.defer(move |cx| cx.notify(entity_id));
        }
    }
}

impl Render for Library {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let grid_width = self.grid_bounds.size.width;

        let num_cols = ((grid_width + MIN_GAP) / (GAME_CARD_WIDTH + MIN_GAP)).floor() as usize;
        let num_cols = num_cols.max(1);
        let num_rows = (self.games.len() as f32 / num_cols as f32).ceil() as usize;

        let games = self.games.clone();


        let card_height = GAME_CARD_WIDTH / ( 2. / 3.);
        let rows = (window.bounds().size.height / card_height + 2.).ceil() as usize;

        let theme = cx.theme();

        div()
            .on_children_prepainted(cx.processor(Self::set_bounds))
            .flex_grow()
            .px(rems(2.))
            .text_color(theme.colors.accent)
            .image_cache(lru_image_cache("game-grid-cache", num_cols * rows))
            .child(
                uniform_list("game-grid", num_rows, move |range, _, _| {
                    range
                        .map(|row_idx| {
                            let start = row_idx * num_cols;
                            let end = (start + num_cols).min(games.len());
                            let row = &games[start..end];

                            let blanks = num_cols - row.len();

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
