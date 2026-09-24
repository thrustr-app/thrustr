use super::Route;
use crate::{
    globals::ComponentRegistryExt,
    navigation::{NavNode, NavSidebar, Navigator, NavigatorExt, Page, SettingsPage, nav_item},
};
use domain::component::Status;
use gpui::{
    AnyView, App, AppContext, Context, EmptyView, IntoElement, ParentElement, Render, Styled,
    Window, div, rems,
};
use ui::{Label, Sidebar, SidebarItem, WithVariant};

mod appearance;
mod config;
mod plugins;
mod storefronts;

pub use appearance::Appearance;
pub use config::Config;
pub use plugins::Plugins;
pub use storefronts::Storefronts;

impl SettingsPage {
    fn build_view(&self, cx: &mut App) -> AnyView {
        match self {
            Self::Storefronts(None) => cx.new(Storefronts::new).into(),
            Self::Plugins(None) => cx.new(Plugins::new).into(),
            Self::Storefronts(Some(id)) | Self::Plugins(Some(id)) => match cx.component(id) {
                Some(component) => cx.new(|cx| Config::new(component, cx)).into(),
                None => cx.new(|_| EmptyView).into(),
            },
            Self::Appearance => cx.new(|_| Appearance).into(),
        }
    }
}

fn settings_item(page: SettingsPage) -> SidebarItem<SettingsPage> {
    let label = page.label();
    nav_item(page).label(label)
}

fn status_label(status: &Status) -> Label {
    match status {
        Status::Initializing => Label::new("INITIALIZING").variant_secondary(),
        Status::Unauthenticated => Label::new("UNAUTHENTICATED").variant_warning(),
        Status::Active => Label::new("ACTIVE").variant_accent(),
        Status::Inactive => Label::new("INACTIVE"),
        Status::Error(_) | Status::InitError(_) => Label::new("ERROR").variant_danger(),
    }
}

pub struct Settings {
    current_page: SettingsPage,
    active_view: AnyView,
}

impl Route for Settings {
    type Args = SettingsPage;
    type State = ();

    fn build(page: SettingsPage, _state: (), _window: &mut Window, cx: &mut Context<Self>) -> Self {
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

impl Render for Settings {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
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
