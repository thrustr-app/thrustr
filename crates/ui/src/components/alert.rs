use gpui::{
    App, FontWeight, IntoElement, ParentElement, Refineable, RenderOnce, SharedString,
    StyleRefinement, Styled, Window, div, prelude::FluentBuilder, rems, svg,
};
use theme_manager::ThemeExt;

#[derive(IntoElement)]
pub struct Alert {
    style: StyleRefinement,
    title: Option<SharedString>,
    description: Option<SharedString>,
}

impl Alert {
    pub fn new() -> Self {
        Self {
            style: StyleRefinement::default(),
            title: None,
            description: None,
        }
    }

    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }
}

impl Styled for Alert {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Alert {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();

        let mut alert = div()
            .flex()
            .w_full()
            .p(rems(1.))
            .rounded(theme.radius.lg)
            .border_2()
            .border_color(theme.colors.error)
            .gap(rems(1.))
            .child(
                svg()
                    .size(rems(1.5))
                    .path("icons/danger.svg")
                    .text_color(theme.colors.error),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .when_some(self.title, |this, title| {
                        this.child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme.colors.error)
                                .child(title),
                        )
                    })
                    .when_some(self.description, |this, description| {
                        this.child(div().text_color(theme.colors.error).child(description))
                    }),
            );

        alert.style().refine(&self.style);
        alert
    }
}
