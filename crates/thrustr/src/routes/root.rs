use crate::components::Sidebar;
use gpui::{Context, IntoElement, ParentElement, Render, Styled, Window, div};
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
            .absolute()
            .size_full()
            .bg(theme.colors.background)
            .flex()
            .child(Sidebar::new())
            .child(div().flex_1())
    }
}
