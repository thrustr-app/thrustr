use crate::{Variant, WithVariant};
use core::panic;
use gpui::{
    Animation, AnimationExt, AnyElement, App, ElementId, FontWeight, Hsla, IntoElement,
    ParentElement, RenderOnce, SharedString, Styled, Window, div, prelude::FluentBuilder,
    pulsating_between, relative, rems,
};
use std::time::Duration;
use theme::{Theme, ThemeExt};

struct Palette {
    background: Option<Hsla>,
    foreground: Hsla,
}

#[derive(IntoElement)]
pub struct Label {
    id: ElementId,
    text: SharedString,
    variant: Variant,
    filled: bool,
    status_dot: Option<bool>,
}

impl Label {
    #[track_caller]
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            id: (ElementId::CodeLocation(*panic::Location::caller()), "label").into(),
            text: text.into(),
            variant: Variant::default(),
            filled: false,
            status_dot: None,
        }
    }

    pub fn filled(mut self) -> Self {
        self.filled = true;
        self
    }

    pub fn status_dot(mut self, pulse: bool) -> Self {
        self.status_dot = Some(pulse);
        self
    }

    fn accent(variant: Variant, theme: &Theme) -> Hsla {
        match variant {
            Variant::Secondary => theme.colors.secondary,
            Variant::Accent => theme.colors.accent,
            Variant::Warning => theme.colors.warning,
            Variant::Danger => theme.colors.danger,
            Variant::Outline | Variant::Ghost => theme.colors.secondary,
        }
    }

    fn palette(variant: Variant, filled: bool, accent: Hsla, theme: &Theme) -> Palette {
        let background = filled
            .then(|| match variant {
                Variant::Secondary => Some(theme.colors.secondary_background),
                Variant::Accent => Some(theme.colors.accent_background),
                Variant::Warning => Some(theme.colors.warning_background),
                Variant::Danger => Some(theme.colors.danger_background),
                Variant::Outline | Variant::Ghost => None,
            })
            .flatten();

        Palette {
            background,
            foreground: accent,
        }
    }

    fn dot(id: impl Into<ElementId>, color: Hsla, pulse: bool) -> AnyElement {
        let dot = div().size(rems(0.375)).rounded_full().bg(color);

        if !pulse {
            return dot.into_any_element();
        }

        dot.with_animation(
            id,
            Animation::new(Duration::from_millis(2400))
                .repeat_synced()
                .with_max_fps(15.)
                .with_easing(pulsating_between(0.35, 1.0)),
            |dot, delta| dot.opacity(delta),
        )
        .into_any_element()
    }
}

impl WithVariant for Label {
    fn variant(mut self, variant: Variant) -> Self {
        self.variant = variant;
        self
    }
}

impl RenderOnce for Label {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let accent = Self::accent(self.variant, &theme);
        let palette = Self::palette(self.variant, self.filled, accent, &theme);

        div()
            .flex()
            .items_center()
            .justify_center()
            .gap(rems(0.375))
            .text_size(theme.text.sm)
            .line_height(relative(1.))
            .font_weight(FontWeight::BOLD)
            .rounded(theme.radius.sm)
            .text_color(palette.foreground)
            .when(self.filled, |label| label.px(rems(0.625)).py(rems(0.25)))
            .when_some(palette.background, |label, background| label.bg(background))
            .when_some(self.status_dot, |label, pulse| {
                label.child(Self::dot((self.id, "status_dot"), accent, pulse))
            })
            .child(self.text)
    }
}
