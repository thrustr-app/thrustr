use domain::game::GameId;
use gpui::{
    Context, IntoElement, ParentElement, Render, Styled, Window, div, linear_color_stop,
    linear_gradient, relative, rems, rgb,
};
use theme::ThemeExt;

const BANNER_HEIGHT_REM: f32 = 28.;

pub struct Game {
    _id: GameId,
}

impl Game {
    pub fn new(id: GameId) -> Self {
        Self { _id: id }
    }
}

impl Render for Game {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        div().flex_grow_1().px(rems(2.)).child(
            div()
                .w_full()
                .h(rems(BANNER_HEIGHT_REM))
                .max_h(relative(0.5))
                .bg(linear_gradient(
                    30.,
                    linear_color_stop(rgb(0x2A7B9B), 0.),
                    linear_color_stop(rgb(0xEDDD53), 1.),
                ))
                .rounded(theme.radius.lg),
        )
    }
}
