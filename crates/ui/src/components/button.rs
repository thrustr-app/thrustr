use crate::{Size, WithSize};
use gpui::{
    AnyElement, App, ClickEvent, Div, ElementId, InteractiveElement, IntoElement, ParentElement,
    Refineable, RenderOnce, Stateful, StatefulInteractiveElement, StyleRefinement, Styled, Window,
    div, prelude::FluentBuilder, rems,
};
use smallvec::SmallVec;
use theme_manager::ThemeExt;

#[derive(IntoElement)]
pub struct Button {
    base: Stateful<Div>,
    style: StyleRefinement,
    size: Size,
    circular: bool,
    children: SmallVec<[AnyElement; 1]>,
    on_click: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>>,
}

impl Button {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            base: div().id(id),
            style: StyleRefinement::default(),
            size: Size::Medium,
            circular: false,
            children: SmallVec::new(),
            on_click: None,
        }
    }

    pub fn circular(mut self) -> Self {
        self.circular = true;
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

impl Styled for Button {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for Button {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl WithSize for Button {
    fn size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }
}

impl RenderOnce for Button {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let mut button = self
            .base
            .when(self.circular, |button| button.rounded_full())
            .when(self.size == Size::Medium, |button| {
                button.size(rems(2.)).p(rems(0.5))
            })
            .border_1()
            .border_color(theme.colors.border)
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .when_some(self.on_click, |button, on_click| button.on_click(on_click))
            .children(self.children);

        button.style().refine(&self.style);
        button
    }
}
