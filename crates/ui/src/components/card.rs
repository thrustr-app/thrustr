use gpui::{
    AnyElement, App, FontWeight, IntoElement, ParentElement, Refineable, RenderOnce, SharedString,
    StyleRefinement, Styled, Window, div, prelude::FluentBuilder, rems,
};
use smallvec::SmallVec;
use theme_manager::ThemeExt;

#[derive(IntoElement)]
pub struct Card {
    style: StyleRefinement,
    title: Option<SharedString>,
    children: SmallVec<[AnyElement; 1]>,
}

impl Card {
    pub fn new() -> Self {
        Self {
            style: StyleRefinement::default(),
            title: None,
            children: SmallVec::new(),
        }
    }

    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }
}

impl Styled for Card {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for Card {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Card {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();

        let mut card = div()
            .bg(theme.colors.card_background)
            .overflow_hidden()
            .p(rems(1.5))
            .rounded(rems(1.5))
            .flex()
            .flex_col()
            .text_color(theme.colors.card_foreground_primary)
            .text_size(rems(1.5))
            .line_height(rems(1.5))
            .gap(rems(1.5))
            .font_weight(FontWeight::SEMIBOLD)
            .when_some(self.title, |card, title| card.child(title))
            .children(self.children);

        card.style().refine(&self.style);
        card
    }
}
