use super::Route;
use crate::navigation::{
    NavNode, NavSidebar, Navigator, NavigatorExt, Page, SettingsPage, nav_item,
};
use gpui::{AnyView, Context, IntoElement, ParentElement, Render, Styled, Window, div, rems};
use ui::{Sidebar, SidebarItem};

mod appearance;
mod config;
mod plugins;
mod storefronts;

pub use appearance::Appearance;
pub use config::Config;
pub use plugins::Plugins;
pub use storefronts::Storefronts;

fn settings_item(page: SettingsPage) -> SidebarItem<SettingsPage> {
    let label = page.label();
    nav_item(page).label(label)
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
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(rems(3.))
            .pb(rems(1.5))
            .flex_grow_1()
            .flex()
            .gap(rems(1.75))
            .child(
                Sidebar::new()
                    .nav(self.current_page.clone())
                    .flex_shrink_0()
                    .item(settings_item(SettingsPage::Storefronts(None)))
                    .item(settings_item(SettingsPage::Plugins(None)))
                    .item(settings_item(SettingsPage::Appearance)),
            )
            .child(self.active_view.clone())
    }
}
