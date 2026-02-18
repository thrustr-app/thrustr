use crate::navigation::{LocationExt, Page, Route, SettingsPage};
use gpui::{
    App, InteractiveElement, IntoElement, ParentElement, RenderOnce, Styled, Window, div,
    prelude::FluentBuilder, rems, svg, transparent_black,
};
use gpui_router::use_location;
use theme_manager::ThemeExt;

#[derive(IntoElement)]
struct SidebarIconButton {
    page: Page,
}

impl SidebarIconButton {
    fn new(page: Page) -> Self {
        Self { page }
    }
}

impl RenderOnce for SidebarIconButton {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let location = use_location(cx);

        self.page
            .nav_link()
            .group(self.page.as_str())
            .p(rems(0.625))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .bg(transparent_black())
            .hover(|div| div.bg(theme.colors.sidebar_highlight))
            .when(self.page == location.page(), |div| {
                div.bg(theme.colors.sidebar_highlight)
            })
            .child(
                svg()
                    .group(self.page.as_str())
                    .path(self.page.icon_path())
                    .text_color(theme.colors.sidebar_foreground_secondary)
                    .size(rems(1.5))
                    .group_hover(self.page.as_str(), |div| {
                        div.text_color(theme.colors.sidebar_foreground_primary)
                    })
                    .when(self.page == location.page(), |svg| {
                        svg.text_color(theme.colors.sidebar_foreground_primary)
                    }),
            )
    }
}

#[derive(IntoElement)]
pub struct Sidebar;

impl Sidebar {
    pub fn new() -> Self {
        Self
    }
}

impl RenderOnce for Sidebar {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();

        let top_nav = div()
            .flex()
            .flex_col()
            .items_center()
            .gap(rems(0.75))
            .child(SidebarIconButton::new(Page::Home))
            .child(SidebarIconButton::new(Page::Library))
            .child(SidebarIconButton::new(Page::Collections));

        let bottom_nav = div()
            .flex()
            .flex_col()
            .items_center()
            .gap(rems(0.75))
            .mb(rems(1.5))
            .child(SidebarIconButton::new(Page::Settings(
                SettingsPage::default(),
            )));

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
                    .child(top_nav)
                    .child(bottom_nav),
            )
    }
}
