use crate::{FocusProps, Icon, Size, Variant, WithFocus, WithSize, WithVariant};
use gpui::{
    Animation, AnimationExt, AnyElement, App, ClickEvent, ElementId, FontWeight, Hsla,
    InteractiveElement, IntoElement, ParentElement, Refineable, Rems, RenderOnce,
    StatefulInteractiveElement, StyleRefinement, Styled, Transformation, Window, div, percentage,
    prelude::FluentBuilder, relative, rems, transparent_black,
};
use smallvec::SmallVec;
use std::time::Duration;
use theme::{Theme, ThemeExt};

#[derive(Clone, Copy)]
struct Palette {
    background: Hsla,
    foreground: Hsla,
    border: Hsla,
    ring: Hsla,
    weight: FontWeight,
}

#[allow(clippy::type_complexity)]
#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    style: StyleRefinement,
    variant: Variant,
    size: Size,
    icon: Option<Icon>,
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
            variant: Variant::default(),
            size: Size::default(),
            icon: None,
            children: SmallVec::new(),
            on_click: None,
            focus: FocusProps::default(),
            loading: false,
            disabled: false,
        }
    }

    pub fn icon(id: impl Into<ElementId>, icon: Icon) -> Self {
        Self {
            icon: Some(icon),
            ..Self::new(id)
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

    fn height(&self) -> Rems {
        if self.variant == Variant::Ghost {
            return rems(1.);
        }

        match self.size {
            Size::Small => rems(2.),
            Size::Medium => rems(2.375),
            Size::Large => rems(2.625),
        }
    }

    fn padding(&self) -> Rems {
        if self.variant == Variant::Ghost {
            return rems(0.25);
        }

        match self.size {
            Size::Small => rems(0.75),
            Size::Medium => rems(1.125),
            Size::Large => rems(1.25),
        }
    }

    fn palette(&self, theme: &Theme) -> Palette {
        let colors = &theme.colors;

        let solid = |background, foreground| Palette {
            background,
            foreground,
            border: transparent_black(),
            ring: colors.primary,
            weight: FontWeight::SEMIBOLD,
        };

        match self.variant {
            Variant::Secondary => solid(colors.secondary, colors.secondary_foreground),
            Variant::Accent => solid(colors.accent, colors.accent_foreground),
            Variant::Warning => solid(colors.warning, colors.warning_foreground),
            Variant::Danger => solid(colors.danger, colors.danger_foreground),
            Variant::Outline => Palette {
                background: transparent_black(),
                foreground: colors.primary,
                border: colors.border,
                ring: colors.primary,
                weight: FontWeight::SEMIBOLD,
            },
            Variant::Ghost => Palette {
                background: transparent_black(),
                foreground: colors.secondary,
                border: transparent_black(),
                ring: colors.secondary,
                weight: FontWeight::NORMAL,
            },
        }
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
        let palette = self.palette(&theme);
        let height = self.height();
        let is_icon = self.icon.is_some();
        let interactive = !self.disabled && !self.loading;

        let mut button = div()
            .id(self.id.clone())
            .flex()
            .items_center()
            .justify_center()
            .h(height)
            .rounded(theme.radius.pill)
            .when(is_icon, |button| button.min_w(height))
            .when(!is_icon, |button| button.px(self.padding()))
            .border_1()
            .border_color(palette.border)
            .bg(palette.background)
            .text_color(palette.foreground)
            .text_size(theme.text.md)
            .line_height(relative(1.))
            .font_weight(palette.weight)
            .focus_visible(move |button| button.border_color(palette.ring))
            .when(self.disabled, |button| button.opacity(0.6))
            .when(interactive, |button| {
                button
                    .track_focus(&focus_handle)
                    .cursor_pointer()
                    .when_some(self.on_click, |button, on_click| button.on_click(on_click))
            })
            .when_else(
                self.loading,
                |button| {
                    button.child(
                        Icon::loader()
                            .size(self.size)
                            .color(palette.foreground)
                            .with_animation(
                                "loading",
                                Animation::new(Duration::from_millis(850))
                                    .repeat()
                                    .with_max_fps(30.),
                                |loader: Icon, delta| {
                                    loader.transform(Transformation::rotate(percentage(delta)))
                                },
                            ),
                    )
                },
                |button| {
                    button
                        .when_some(self.icon, |button, icon| {
                            button.child(icon.size(self.size).color(palette.foreground))
                        })
                        .children(self.children)
                },
            );

        button.style().refine(&self.style);

        self.focus
            .attach_reveal(button, &focus_handle, (self.id, "reveal"), window, cx)
    }
}
