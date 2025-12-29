use gpui::{BoxShadow, Context, IntoElement, ParentElement, Render, Styled, Window, div, rems};
use theme_manager::ThemeExt;

pub struct Root {}

impl Root {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self {}
    }
}

impl Render for Root {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.colors.sidebar_background)
            .child(div().w_full().h(rems(3.5)))
            .child(
                div()
                    .flex()
                    .flex_grow()
                    .child(div().flex_shrink_0().h_full().w(rems(4.625)))
                    .child(
                        div()
                            .flex_grow()
                            .size_full()
                            .flex_grow()
                            .bg(theme.colors.background)
                            .rounded_tl(rems(1.25))
                            .border_color(theme.colors.border)
                            .border_t_1()
                            .border_l_1(),
                    ),
            )
            .flex_grow()
    }
}
