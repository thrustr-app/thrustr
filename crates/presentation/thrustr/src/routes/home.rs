use gpui::{Context, IntoElement, Render, Styled, Window, div};
use theme_manager::ThemeExt;

pub struct Home;

impl Render for Home {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _theme = cx.theme();
        div().flex_grow()
    }
}
