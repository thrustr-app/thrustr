use crate::navigation::{NavigatorExt, Page};
use gpui::{
    App, InteractiveElement, IntoElement, ParentElement, RenderOnce, StatefulInteractiveElement,
    Styled, Window, div, prelude::FluentBuilder, rems, svg, transparent_black,
};
use theme::ThemeExt;

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

        let target_page = self.page.clone();
        let label = self.page.label();
        let is_active = cx.navigator().is_active_for(self.page.clone());

        div()
            .id(label)
            .cursor_pointer()
            .on_click(move |_, _, cx| cx.navigate(target_page.clone()))
            .group(label)
            .p(rems(0.625))
            .flex()
            .items_center()
            .justify_center()
            .rounded(theme.radius.full)
            .bg(transparent_black())
            .hover(|div| div.bg(theme.colors.sidebar_surface))
            .when(is_active, |div| div.bg(theme.colors.sidebar_surface))
            .child(
                svg()
                    .group(label)
                    .path(self.page.icon_path())
                    .text_color(theme.colors.sidebar_secondary)
                    .size(rems(1.5))
                    .group_hover(label, |div| div.text_color(theme.colors.sidebar_primary))
                    .when(is_active, |svg| {
                        svg.text_color(theme.colors.sidebar_primary)
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
            .child(SidebarIconButton::new(Page::Settings(None)));

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
                    .text_color(theme.colors.sidebar_logo)
                    .mt(rems(1.5))
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
