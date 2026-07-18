use crate::extensions::SpawnTaskExt;
use crate::globals::GameServiceExt;
use domain::game::GameId;
use gpui::{
    Context, FontWeight, IntoElement, ParentElement, Render, SharedString, Styled, Window, div,
    linear_color_stop, linear_gradient, relative, rems, rgb,
};
use theme::ThemeExt;
use tracing::error;

const BANNER_HEIGHT_REM: f32 = 28.;

pub struct Game {
    _id: GameId,
    name: SharedString,
    summary: Option<SharedString>,
}

impl Game {
    pub fn new(id: GameId, cx: &mut Context<Self>) -> Self {
        let game_service = cx.game_service();
        cx.spawn_and_update(
            async move { game_service.get(id) },
            |game, result, _| match result {
                Ok(Some(loaded)) => {
                    game.name = loaded.name.into();
                    game.summary = loaded.summary.map(Into::into);
                }
                Ok(None) => {}
                Err(e) => {
                    error!("failed to load game: {e:#}");
                }
            },
        );

        Self {
            _id: id,
            name: SharedString::default(),
            summary: None,
        }
    }
}

impl Render for Game {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        div().flex_grow_1().min_w_0().px(rems(2.)).child(
            div()
                .w_full()
                .overflow_hidden()
                .h(rems(BANNER_HEIGHT_REM))
                .max_h(relative(0.5))
                .bg(linear_gradient(
                    30.,
                    linear_color_stop(rgb(0x2A7B9B), 0.),
                    linear_color_stop(rgb(0xEDDD53), 1.),
                ))
                .rounded(theme.radius.lg)
                .p(rems(2.))
                .flex()
                .flex_col()
                .justify_end()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .w_full()
                        .gap(rems(0.5))
                        .child(
                            div()
                                .w_full()
                                .text_size(rems(2.5))
                                .font_weight(FontWeight::BOLD)
                                .truncate()
                                .child(self.name.clone()),
                        )
                        .children(self.summary.clone().map(|summary| {
                            div()
                                .w_full()
                                .text_size(rems(1.))
                                .line_clamp(3)
                                .child(summary)
                        })),
                ),
        )
    }
}
