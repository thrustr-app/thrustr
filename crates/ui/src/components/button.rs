use crate::{Size, Variant, WithSize, WithVariant};
use gpui::{
    AnyElement, App, ClickEvent, ElementId, InteractiveElement, IntoElement, ParentElement,
    Refineable, RenderOnce, StatefulInteractiveElement, StyleRefinement, Styled, Window, div,
    prelude::FluentBuilder, rems, transparent_black,
};
use smallvec::SmallVec;
use theme_manager::ThemeExt;

#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    style: StyleRefinement,
    variant: Variant,
    size: Size,
    circular: bool,
    children: SmallVec<[AnyElement; 1]>,
    on_click: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>>,
    auto_focus: bool,
    tab_index: isize,
    tab_stop: bool,
}

impl Button {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: (id.into(), "button").into(),
            style: StyleRefinement::default(),
            variant: Variant::Accent,
            size: Size::Medium,
            circular: false,
            children: SmallVec::new(),
            on_click: None,
            auto_focus: false,
            tab_index: 0,
            tab_stop: true,
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

    pub fn tab_stop(mut self, tab_stop: bool) -> Self {
        self.tab_stop = tab_stop;
        self
    }

    pub fn tab_index(mut self, tab_index: isize) -> Self {
        self.tab_index = tab_index;
        self
    }

    pub fn auto_focus(mut self, auto_focus: bool) -> Self {
        self.auto_focus = auto_focus;
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

impl WithVariant for Button {
    fn variant(mut self, variant: Variant) -> Self {
        self.variant = variant;
        self
    }
}

impl RenderOnce for Button {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let mut focus_handle = window
            .use_keyed_state(self.id.clone(), cx, |window, app| {
                let focus_handle = app.focus_handle();
                if self.auto_focus {
                    focus_handle.focus(window);
                }
                focus_handle
            })
            .read(cx)
            .clone();

        if focus_handle.tab_stop != self.tab_stop {
            focus_handle = focus_handle.tab_stop(self.tab_stop);
        }
        if focus_handle.tab_index != self.tab_index {
            focus_handle = focus_handle.tab_index(self.tab_index);
        }

        let theme = cx.theme();

        let mut button = div()
            .id(self.id)
            .track_focus(&focus_handle)
            .rounded(rems(0.5))
            .when(self.circular, |button| button.rounded_full())
            .when(self.size == Size::Medium, |button| {
                button
                    .h(rems(2.25))
                    .min_w(rems(2.25))
                    .p(rems(0.6))
                    .text_size(rems(1.))
            })
            .when(self.size == Size::Large, |button| {
                button
                    .h(rems(2.5))
                    .min_w(rems(2.5))
                    .p(rems(0.625))
                    .text_size(rems(1.))
            })
            .border_1()
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .when_some(self.on_click, |button, on_click| button.on_click(on_click))
            .children(self.children);

        match self.variant {
            Variant::Accent => {
                button = button
                    .bg(theme.colors.accent)
                    .text_color(theme.colors.background)
                    .focus(|input| input.border_color(theme.colors.foreground_primary));
            }
            Variant::Ghost => {
                button = button
                    .bg(transparent_black())
                    .border_color(theme.colors.border)
                    .focus(|input| input.border_color(theme.colors.foreground_primary));
            }
        }

        button.style().refine(&self.style);
        button
    }
}
