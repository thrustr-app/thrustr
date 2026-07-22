use crate::app::Route;
use gpui::{Context, IntoElement, Render, Styled, Window, div};
use theme::ThemeExt;

pub struct Home;

impl Route for Home {}

impl Render for Home {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _theme = cx.theme();
        div().flex_grow_1()
    }
}
