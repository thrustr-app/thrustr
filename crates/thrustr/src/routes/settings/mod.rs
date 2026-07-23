use crate::{
    app::Route,
    navigation::{NavNode, Navigator, NavigatorExt, Page, SettingsPage, nav_item},
};
use gpui::{AnyView, App, Context, IntoElement, ParentElement, Render, Styled, Window, div, rems};
use theme::ThemeExt;
use ui::{Sidebar, SidebarItem, SidebarPalette};

mod appearance;
mod config;
mod plugins;
mod storefronts;

pub use appearance::Appearance;
pub use config::Config;
pub use plugins::Plugins;
pub use storefronts::Storefronts;

fn settings_item(page: SettingsPage, cx: &App) -> SidebarItem {
    let label = page.label();
    nav_item(page, cx).label(label)
}

pub struct Settings {
    current_page: SettingsPage,
    active_view: AnyView,
}

impl Settings {
    pub fn new(page: SettingsPage, cx: &mut Context<Self>) -> Self {
        let active_view = page.build_view(cx);

        cx.observe_global::<Navigator>(|this, cx| {
            if let Page::Settings(Some(page)) = cx.navigator().current_page()
                && page != this.current_page
            {
                this.current_page = page.clone();
                this.active_view = page.build_view(cx);
                cx.notify();
            }
        })
        .detach();

        Self {
            current_page: page,
            active_view,
        }
    }
}

impl Route for Settings {}

impl Render for Settings {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .px(rems(1.5))
            .pb(rems(1.5))
            .flex_grow_1()
            .flex()
            .child(
                Sidebar::new("settings-sidebar")
                    .palette(SidebarPalette::Content)
                    .flex_shrink_0()
                    .pr(rems(1.5))
                    .border_r_1()
                    .border_color(theme.colors.border)
                    .item(settings_item(SettingsPage::Storefronts(None), cx))
                    .item(settings_item(SettingsPage::Plugins(None), cx))
                    .item(settings_item(SettingsPage::Appearance, cx)),
            )
            .child(self.active_view.clone())
    }
}
