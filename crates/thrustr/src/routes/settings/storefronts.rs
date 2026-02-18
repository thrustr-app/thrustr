use gpui::{Context, IntoElement, ParentElement, Render, Styled, Window, div, rems};
use theme_manager::ThemeExt;
use ui::Card;

pub struct Storefronts;

impl Render for Storefronts {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _theme = cx.theme();
        div()
            .flex_grow()
            .px(rems(1.5))
            .child(Card::new().title("Epic Games").size(rems(10.)))
    }
}
