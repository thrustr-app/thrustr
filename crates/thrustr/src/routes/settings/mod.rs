use crate::navigation::{LocationExt, Route, SettingsPage};
use gpui::{
    App, InteractiveElement, IntoElement, ParentElement, RenderOnce, Styled, Window, div,
    prelude::FluentBuilder, rems, svg, transparent_black,
};
use gpui_router::{IntoLayout, Outlet, use_location};
use theme_manager::ThemeExt;

mod appearance;
mod plugins;
mod storefronts;

pub use appearance::Appearance;
pub use plugins::Plugins;
pub use storefronts::Storefronts;

#[derive(IntoElement)]
struct SettingsPageButton {
    page: SettingsPage,
}

impl SettingsPageButton {
    fn new(page: SettingsPage) -> Self {
        Self { page }
    }
}

impl RenderOnce for SettingsPageButton {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let location = use_location(cx);

        self.page
            .nav_link()
            .py(rems(0.625))
            .px(rems(1.25))
            .w_full()
            .flex()
            .items_center()
            .rounded_full()
            .gap(rems(0.75))
            .bg(transparent_black())
            .hover(|style| style.bg(theme.colors.sidebar_highlight))
            .when(Some(self.page) == location.settings_page(), |style| {
                style
                    .bg(theme.colors.sidebar_highlight)
                    .text_color(theme.colors.sidebar_foreground_primary)
            })
            .child(
                svg()
                    .flex_shrink_0()
                    .path(self.page.icon_path())
                    .text_color(theme.colors.sidebar_foreground_secondary)
                    .size(rems(1.5))
                    .when(Some(self.page) == location.settings_page(), |svg| {
                        svg.text_color(theme.colors.sidebar_foreground_primary)
                    }),
            )
            .child(div().child(self.page.as_str_pretty()))
    }
}

#[derive(IntoElement, IntoLayout)]
pub struct SettingsLayout {
    outlet: Outlet,
}

impl SettingsLayout {
    pub fn new() -> Self {
        Self {
            outlet: Outlet::new(),
        }
    }
}

impl RenderOnce for SettingsLayout {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .px(rems(1.5))
            .pb(rems(1.5))
            .flex_grow()
            .flex()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(rems(0.75))
                    .items_center()
                    .flex_shrink_0()
                    .w_auto()
                    .pr(rems(1.5))
                    .border_r_1()
                    .border_color(theme.colors.border)
                    .text_color(theme.colors.sidebar_foreground_secondary)
                    .child(SettingsPageButton::new(SettingsPage::Storefronts))
                    .child(SettingsPageButton::new(SettingsPage::Plugins))
                    .child(SettingsPageButton::new(SettingsPage::Appearance)),
            )
            .child(self.outlet)
    }
}
