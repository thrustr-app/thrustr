use gpui::{Context, IntoElement, Render, Window, div};
use theme_manager::ThemeExt;

pub struct Root {}

impl Root {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self {}
    }
}

impl Render for Root {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _theme = cx.theme();
        div()
    }
}
