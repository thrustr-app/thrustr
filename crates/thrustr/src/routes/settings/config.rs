use gpui::{Context, IntoElement, ParentElement, Render, SharedString, Styled, Window, div};
use theme_manager::ThemeExt;

pub struct Config {
    component_id: SharedString,
}

impl Config {
    pub fn new(component_id: impl Into<SharedString>) -> Self {
        Self {
            component_id: component_id.into(),
        }
    }
}

impl Render for Config {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .flex_grow()
            .child(self.component_id.clone())
            .text_color(theme.colors.accent)
    }
}
