use crate::{FocusProps, Size, Variant, WithFocus, WithSize, WithVariant};
use gpui::{
    Animation, AnimationExt, AnyElement, App, ClickEvent, ElementId, InteractiveElement,
    IntoElement, ParentElement, Refineable, RenderOnce, StatefulInteractiveElement,
    StyleRefinement, Styled, Transformation, Window, div, percentage, prelude::FluentBuilder, rems,
    svg, transparent_black,
};
use smallvec::SmallVec;
use std::time::Duration;
use theme::ThemeExt;

#[allow(clippy::type_complexity)]
#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    style: StyleRefinement,
    variant: Variant,
    size: Size,
    children: SmallVec<[AnyElement; 1]>,
    on_click: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>>,
    focus: FocusProps,
    loading: bool,
    disabled: bool,
}

impl Button {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: (id.into(), "button").into(),
            style: StyleRefinement::default(),
            variant: Variant::Primary,
            size: Size::Medium,
            children: SmallVec::new(),
            on_click: None,
            focus: FocusProps::default(),
            loading: false,
            disabled: false,
        }
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }

    pub fn loading(mut self) -> Self {
        self.loading = true;
        self
    }

    pub fn disabled(mut self) -> Self {
        self.disabled = true;
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

impl WithFocus for Button {
    fn focus_props(&mut self) -> &mut FocusProps {
        &mut self.focus
    }
}

impl RenderOnce for Button {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let focus_handle = window
            .use_keyed_state(self.id.clone(), cx, |window, cx| {
                let focus_handle = cx.focus_handle();
                if self.focus.auto_focus {
                    focus_handle.focus(window, cx);
                }
                focus_handle
            })
            .read(cx)
            .clone();
        let focus_handle = self.focus.configure(focus_handle);

        let theme = cx.theme();

        let mut button = div()
            .id(self.id.clone())
            .rounded(theme.radius.full)
            .when(!self.disabled && !self.loading, |button| {
                button
                    .track_focus(&focus_handle)
                    .cursor_pointer()
                    .when_some(self.on_click, |button, on_click| button.on_click(on_click))
            })
            .when(self.disabled, |button| button.opacity(0.6))
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
            .when_else(
                self.loading,
                |button| {
                    button.child(
                        svg()
                            .path("icons/loader.svg")
                            .size(rems(1.25))
                            .text_color(theme.colors.background)
                            .when(self.variant == Variant::Ghost, |svg| {
                                svg.text_color(theme.colors.primary)
                            })
                            .with_animation(
                                "loading",
                                Animation::new(Duration::from_millis(850)).repeat(),
                                |loader, delta| {
                                    loader.with_transformation(Transformation::rotate(percentage(
                                        delta,
                                    )))
                                },
                            ),
                    )
                },
                |button| button.children(self.children),
            );

        match self.variant {
            Variant::Primary => {
                button = button
                    .bg(theme.colors.primary)
                    .text_color(theme.colors.background)
                    .focus_visible(|input| input.border_color(theme.colors.accent))
            }
            Variant::Accent => {
                button = button
                    .bg(theme.colors.accent)
                    .text_color(theme.colors.background)
                    .focus_visible(|input| input.border_color(theme.colors.primary));
            }
            Variant::Ghost => {
                button = button
                    .bg(transparent_black())
                    .text_color(theme.colors.primary)
                    .border_color(theme.colors.border)
                    .focus_visible(|input| input.border_color(theme.colors.primary));
            }
            _ => {}
        }

        button.style().refine(&self.style);

        self.focus
            .attach_reveal(button, &focus_handle, (self.id, "reveal"), window, cx)
    }
}
