use gpui::{Context, IntoElement, Render, Styled, Window, div};

pub struct Plugins {}

impl Plugins {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {}
    }
}

impl Render for Plugins {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().flex_grow()
    }
}
