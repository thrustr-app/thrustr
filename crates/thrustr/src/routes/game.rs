use crate::globals::GameServiceExt;
use domain::game::GameId;
use gpui::{
    App, Context, FontWeight, IntoElement, ParentElement, Render, SharedString, Styled, Window,
    div, linear_color_stop, linear_gradient, relative, rems, rgb,
};
use theme::ThemeExt;

const BANNER_HEIGHT_REM: f32 = 28.;

pub struct Game {
    _id: GameId,
    name: SharedString,
    description: Option<SharedString>,
}

impl Game {
    pub fn new(id: GameId, cx: &mut App) -> Self {
        let game = cx.game_service().get(id).ok().flatten();
        let (name, description) = match game {
            Some(game) => (game.name.into(), game.description.map(Into::into)),
            None => (SharedString::default(), None),
        };

        Self {
            _id: id,
            name,
            description,
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
                        .children(self.description.clone().map(|desc| {
                            div().w_full().text_size(rems(1.)).line_clamp(3).child(desc)
                        })),
                ),
        )
    }
}
