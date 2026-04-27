use crate::navigation::{NavigatorExt, SettingsPage};
use gpui::{
    AnyView, App, Context, InteractiveElement, IntoElement, ParentElement, Render, RenderOnce,
    StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, rems, svg,
    transparent_black,
};
use theme::ThemeExt;

mod appearance;
mod config;
mod plugins;
mod storefronts;

pub use appearance::Appearance;
pub use config::Config;
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

        let target = self.page.clone();
        let is_active = cx.navigator().is_active_for(self.page.clone());

        div()
            .id(self.page.label())
            .cursor_pointer()
            .on_click(move |_, _, cx| cx.navigate(target.clone()))
            .py(rems(0.625))
            .px(rems(1.25))
            .w_full()
            .flex()
            .items_center()
            .rounded(theme.radius.full)
            .gap(rems(0.75))
            .bg(transparent_black())
            .hover(|style| style.bg(theme.colors.surface))
            .when(is_active, |style| {
                style
                    .bg(theme.colors.surface)
                    .text_color(theme.colors.primary)
            })
            .child(
                svg()
                    .flex_shrink_0()
                    .path(self.page.icon_path())
                    .text_color(theme.colors.secondary)
                    .size(rems(1.5))
                    .when(is_active, |svg| svg.text_color(theme.colors.primary)),
            )
            .child(div().child(self.page.label()))
    }
}

pub struct Settings {
    active_view: AnyView,
}

impl Settings {
    pub fn new(page: SettingsPage, cx: &mut Context<Self>) -> Self {
        let active_view = page.build_view(cx);

        Self { active_view }
    }
}

impl Render for Settings {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
                    .text_color(theme.colors.secondary)
                    .child(SettingsPageButton::new(SettingsPage::Storefronts(None)))
                    .child(SettingsPageButton::new(SettingsPage::Plugins(None)))
                    .child(SettingsPageButton::new(SettingsPage::Appearance)),
            )
            .child(self.active_view.clone())
    }
}
