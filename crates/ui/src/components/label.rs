use crate::{Variant, WithVariant};
use gpui::{
    App, IntoElement, ParentElement, RenderOnce, SharedString, Styled, Window, div,
    prelude::FluentBuilder, rems,
};
use theme::ThemeExt;

#[derive(IntoElement)]
pub struct Label {
    text: SharedString,
    variant: Variant,
}

impl Label {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            variant: Variant::Primary,
        }
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

        let label = div()
            .child(self.text)
            .text_size(rems(0.6))
            .p(rems(0.3))
            .flex()
            .items_center()
            .justify_center()
            .rounded(theme.radius.sm)
            .when(self.variant == Variant::Secondary, |label| {
                label
                    .bg(theme.colors.secondary)
                    .text_color(theme.colors.background)
            })
            .when(self.variant == Variant::Accent, |label| {
                label
                    .bg(theme.colors.accent)
                    .text_color(theme.colors.background)
            })
            .when(self.variant == Variant::Warning, |label| {
                label
                    .bg(theme.colors.warning)
                    .text_color(theme.colors.background)
            })
            .when(self.variant == Variant::Destructive, |label| {
                label
                    .bg(theme.colors.error)
                    .text_color(theme.colors.primary)
            });

        label
    }
}
