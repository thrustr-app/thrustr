use gpui::{
    App, InteractiveElement, IntoElement, ParentElement, RenderOnce, SharedString, Styled, Window,
    div, prelude::FluentBuilder, rems, svg, transparent_black,
};
use gpui_router::{NavLink, use_location};
use theme_manager::ThemeExt;

#[derive(IntoElement)]
struct SidebarIconButton {
    path: SharedString,
    icon_path: SharedString,
}

impl SidebarIconButton {
    fn new(path: impl Into<SharedString>, icon_path: impl Into<SharedString>) -> Self {
        Self {
            path: path.into(),
            icon_path: icon_path.into(),
        }
    }

    fn first_component(path: &str) -> &str {
        if path == "/" {
            return "/";
        }

        match path[1..].find('/') {
            Some(i) => &path[..=i],
            None => path,
        }
    }
}

impl RenderOnce for SidebarIconButton {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();

        let location = use_location(cx);
        let base_path = Self::first_component(&location.pathname);

        NavLink::new()
            .to(self.path.clone())
            .group(self.icon_path.clone())
            .size(rems(2.75))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .bg(transparent_black())
            .hover(|div| div.bg(theme.colors.sidebar_highlight))
            .child(
                svg()
                    .group(self.icon_path.clone())
                    .path(self.icon_path.clone())
                    .text_color(theme.colors.sidebar_foreground_secondary)
                    .size(rems(1.5))
                    .group_hover(self.icon_path, |div| {
                        div.text_color(theme.colors.sidebar_foreground_primary)
                    })
                    .when(self.path == base_path, |svg| {
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
            .child(SidebarIconButton::new("/", "icons/home.svg"))
            .child(SidebarIconButton::new("/library", "icons/library.svg"))
            .child(SidebarIconButton::new(
                "/collections",
                "icons/collections.svg",
            ));

        let bottom_nav = div()
            .flex()
            .flex_col()
            .items_center()
            .gap(rems(0.75))
            .mb(rems(1.5))
            .child(SidebarIconButton::new("/settings", "icons/settings.svg"));

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
