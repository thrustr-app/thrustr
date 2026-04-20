use crate::{extensions::SpawnTaskExt, globals::GameServiceExt};
use game::GameListItem;
use gpui::{
    App, Bounds, Context, IntoElement, ParentElement, Pixels, Render, RenderOnce, SharedString,
    Styled, Window, div, px, rems, uniform_list,
};
use std::rc::Rc;
use theme::ThemeExt;

const GAME_CARD_WIDTH: Pixels = px(200.);
const MIN_GAP: Pixels = px(16.);

#[derive(Clone)]
struct GameEntry {
    name: SharedString,
}

#[derive(IntoElement)]
struct GameCard {
    game: GameEntry,
}

impl GameCard {
    fn new(game: GameEntry) -> Self {
        Self { game }
    }
}

impl RenderOnce for GameCard {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();

        div()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap(rems(0.5))
            .w(GAME_CARD_WIDTH)
            .child(
                div()
                    .w_full()
                    .aspect_ratio(2. / 3.)
                    .bg(theme.colors.card_background)
                    .rounded(theme.radius.lg),
            )
            .overflow_hidden()
            .whitespace_nowrap()
            .text_color(theme.colors.primary)
            .child(self.game.name)
    }
}

pub struct Library {
    games: Rc<Vec<GameEntry>>,
    grid_bounds: Bounds<Pixels>,
}

impl Library {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut page = Self {
            games: Rc::new(Vec::new()),
            grid_bounds: Bounds::default(),
        };

        page.refresh_games(cx);

        page
    }

    fn refresh_games(&mut self, cx: &mut Context<Self>) {
        let game_service = cx.game_service();

        cx.spawn_and_update(
            async move { game_service.list(0, 999999) },
            |library, result, _| {
                match result {
                    Ok(games) => {
                        library.games = Rc::new(games.into_iter().map(Into::into).collect());
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let grid_width = self.grid_bounds.size.width;

        let num_cols = ((grid_width + MIN_GAP) / (GAME_CARD_WIDTH + MIN_GAP)).floor() as usize;
        let num_cols = num_cols.max(1);
        let num_rows = (self.games.len() as f32 / num_cols as f32).ceil() as usize;

        let games = self.games.clone();

        let theme = cx.theme();

        div()
            .on_children_prepainted(cx.processor(Self::set_bounds))
            .flex_grow()
            .px(rems(2.))
            .text_color(theme.colors.accent)
            .child(
                uniform_list("game-grid", num_rows, move |range, _, _| {
                    range
                        .map(|row_idx| {
                            let start = row_idx * num_cols;
                            let end = (start + num_cols).min(games.len());
                            let row = &games[start..end];

                            div()
                                .w_full()
                                .flex()
                                .justify_between()
                                .pb(rems(2.))
                                .children(row.iter().map(|game| GameCard::new(game.clone())))
                        })
                        .collect()
                })
                .size_full(),
            )
    }
}

impl From<GameListItem> for GameEntry {
    fn from(entry: GameListItem) -> Self {
        Self {
            name: entry.name.into(),
        }
    }
}
