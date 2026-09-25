use super::Route;
use gpui::{Context, IntoElement, Render, Styled, Window, div};
use theme::ThemeExt;

pub struct Collections;

impl Route for Collections {
    type Args = ();
    type State = ();

    fn build(_args: (), _state: (), _window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Collections
    }
}

impl Render for Collections {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _theme = cx.theme();
        div().flex_grow_1()
    }
}
