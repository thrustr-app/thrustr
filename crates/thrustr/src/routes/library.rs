use gpui::{Context, IntoElement, Render, Styled, Window, div, red};
use theme_manager::ThemeExt;

pub struct Library;

impl Render for Library {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _theme = cx.theme();
        div().bg(red()).flex_grow()
    }
}
