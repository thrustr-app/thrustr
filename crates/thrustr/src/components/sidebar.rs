use gpui::{
    App, InteractiveElement, IntoElement, ParentElement, RenderOnce, Styled, Window, blue, div,
    green, red, rems, svg, transparent_black,
};
use theme_manager::ThemeExt;

#[derive(IntoElement)]
pub struct Sidebar;

impl Sidebar {
    pub fn new() -> Self {
        Self {}
    }
}

impl RenderOnce for Sidebar {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();

        div()
            .flex()
            .flex_col()
            .gap(rems(2.))
            .items_center()
            .flex_shrink_0()
            .w(rems(5.5))
            .bg(theme.colors.sidebar_background)
            .border_r_1()
            .border_color(theme.colors.border)
            .child(
                svg()
                    .path("icons/logo.svg")
                    .text_color(theme.colors.logo)
                    .mt(rems(1.25))
                    .size(rems(3.)),
            )
            .child(
                div()
                    .flex()
                    .flex_grow()
                    .flex_col()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(rems(0.75))
                            .child(
                                div()
                                    .group("home")
                                    .size(rems(2.75))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_full()
                                    .bg(transparent_black())
                                    .hover(|div| div.bg(theme.colors.sidebar_highlight))
                                    .child(
                                        svg()
                                            .group("home")
                                            .path("icons/home.svg")
                                            .text_color(theme.colors.sidebar_foreground_secondary)
                                            .size(rems(1.5))
                                            .group_hover("home", |div| {
                                                div.text_color(
                                                    theme.colors.sidebar_foreground_primary,
                                                )
                                            }),
                                    ),
                            )
                            .child(
                                div()
                                    .group("library")
                                    .size(rems(2.75))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_full()
                                    .bg(transparent_black())
                                    .hover(|div| div.bg(theme.colors.sidebar_highlight))
                                    .child(
                                        svg()
                                            .group("library")
                                            .path("icons/library.svg")
                                            .text_color(theme.colors.sidebar_foreground_secondary)
                                            .size(rems(1.5))
                                            .group_hover("library", |div| {
                                                div.text_color(
                                                    theme.colors.sidebar_foreground_primary,
                                                )
                                            }),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .mb(rems(1.5))
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(rems(0.75))
                            .child(
                                div()
                                    .group("settings")
                                    .size(rems(2.75))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_full()
                                    .bg(transparent_black())
                                    .hover(|div| div.bg(theme.colors.sidebar_highlight))
                                    .child(
                                        svg()
                                            .group("settings")
                                            .path("icons/settings.svg")
                                            .text_color(theme.colors.sidebar_foreground_secondary)
                                            .size(rems(1.5))
                                            .group_hover("settings", |div| {
                                                div.text_color(
                                                    theme.colors.sidebar_foreground_primary,
                                                )
                                            }),
                                    ),
                            ),
                    ),
            )
    }
}
