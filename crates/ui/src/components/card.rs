use gpui::{
    AnyElement, App, FontWeight, IntoElement, ParentElement, Refineable, RenderOnce, SharedString,
    StyleRefinement, Styled, Window, div, prelude::FluentBuilder, rems,
};
use smallvec::SmallVec;
use theme_manager::ThemeExt;

#[derive(IntoElement)]
pub struct Card {
    style: StyleRefinement,
    header: Option<AnyElement>,
    children: SmallVec<[AnyElement; 1]>,
}

impl Card {
    pub fn new() -> Self {
        Self {
            style: StyleRefinement::default(),
            header: None,
            children: SmallVec::new(),
        }
    }

    pub fn title(self, title: impl Into<SharedString>) -> Self {
        self.header(
            div()
                .text_size(rems(1.5))
                .line_height(rems(1.5))
                .font_weight(FontWeight::SEMIBOLD)
                .child(title.into()),
        )
    }

    pub fn header(mut self, header: impl IntoElement) -> Self {
        self.header = Some(header.into_any_element());
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
            .when_some(self.header, |card, header| card.child(header))
            .children(self.children);

        card.style().refine(&self.style);
        card
    }
}
