use gpui::{Context, IntoElement, Render, Styled, Window, blue, div};
use theme_manager::ThemeExt;

pub struct Settings;

impl Render for Settings {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _theme = cx.theme();
        div().bg(blue()).flex_grow()
    }
}
