use gpui::{
    AnyElement, App, ClickEvent, ElementId, FontWeight, InteractiveElement, IntoElement,
    ParentElement, Refineable, RenderOnce, SharedString, StatefulInteractiveElement,
    StyleRefinement, Styled, Window, div, prelude::FluentBuilder, rems,
};
use smallvec::SmallVec;
use theme_manager::ThemeExt;

#[derive(IntoElement)]
pub struct Card {
    id: ElementId,
    style: StyleRefinement,
    header: Option<AnyElement>,
    children: SmallVec<[AnyElement; 1]>,
    on_click: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>>,
}

impl Card {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            style: StyleRefinement::default(),
            header: None,
            children: SmallVec::new(),
            on_click: None,
        }
    }

    pub fn title(self, title: impl Into<SharedString>) -> Self {
        self.header(
            div()
                .text_size(rems(1.25))
                .line_height(rems(1.25))
                .font_weight(FontWeight::MEDIUM)
                .child(title.into()),
        )
    }

    pub fn header(mut self, header: impl IntoElement) -> Self {
        self.header = Some(header.into_any_element());
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
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
            .id((self.id.clone(), "card"))
            .bg(theme.colors.card_background)
            .overflow_hidden()
            .p(rems(1.5))
            .rounded(rems(1.5))
            .flex()
            .gap(rems(1.5))
            .flex_col()
            .mt_0()
            .text_color(theme.colors.card_primary)
            .when_some(self.header, |card, header| card.child(header))
            .when_some(self.on_click, |card, on_click| {
                card.cursor_pointer().on_click(on_click)
            })
            .children(self.children);

        card.style().refine(&self.style);
        card
    }
}
