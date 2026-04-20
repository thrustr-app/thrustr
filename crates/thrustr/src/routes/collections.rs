use gpui::{Context, IntoElement, Render, Styled, Window, div};
use theme::ThemeExt;

pub struct Collections;

impl Render for Collections {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _theme = cx.theme();
        div().flex_grow()
    }
}
