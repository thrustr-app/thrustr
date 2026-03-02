use crate::navigation::{self, Navigator, Page, SettingsPage};
use gpui::{
    AnyView, App, ClickEvent, Context, InteractiveElement, IntoElement, ParentElement, Render,
    RenderOnce, StatefulInteractiveElement, Styled, Subscription, Window, div,
    prelude::FluentBuilder, rems, svg, transparent_black,
};
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
        let current_settings = navigation::current_settings_page(cx);
        let target = self.page;

        div()
            .id(self.page.label())
            .cursor_pointer()
            .on_click(
                move |_event: &ClickEvent, _window: &mut Window, cx: &mut App| {
                    navigation::navigate(cx, Page::Settings(target));
                },
            )
            .py(rems(0.625))
            .px(rems(1.25))
            .w_full()
            .flex()
            .items_center()
            .rounded_full()
            .gap(rems(0.75))
            .bg(transparent_black())
            .hover(|style| style.bg(theme.colors.sidebar_highlight))
            .when(current_settings == Some(self.page), |style| {
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
                    .when(current_settings == Some(self.page), |svg| {
                        svg.text_color(theme.colors.sidebar_foreground_primary)
                    }),
            )
            .child(div().child(self.page.label_pretty()))
    }
}

pub struct Settings {
    active_view: AnyView,
    _subscriptions: Vec<Subscription>,
}

impl Settings {
    pub fn new(sub: SettingsPage, cx: &mut Context<Self>) -> Self {
        let active_view = sub.build_view(cx);

        let subscription = cx.observe_global::<Navigator>(|this, cx| {
            if let Some(sub) = navigation::current_settings_page(cx) {
                this.active_view = sub.build_view(cx);
                cx.notify();
            }
        });

        Self {
            active_view,
            _subscriptions: vec![subscription],
        }
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
                    .text_color(theme.colors.sidebar_foreground_secondary)
                    .child(SettingsPageButton::new(SettingsPage::Storefronts))
                    .child(SettingsPageButton::new(SettingsPage::Plugins))
                    .child(SettingsPageButton::new(SettingsPage::Appearance)),
            )
            .child(self.active_view.clone())
    }
}
