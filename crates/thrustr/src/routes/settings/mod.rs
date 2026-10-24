use super::Route;
use crate::{
    context::EventListenerExt,
    globals::ComponentRegistryExt,
    navigation::{NavNode, NavSidebar, Navigator, NavigatorExt, Page, SettingsPage, nav_item},
};
use domain::component::{InactiveReason, Status};
use event::Topic;
use gpui::{
    AnyView, App, AppContext, Context, IntoElement, ParentElement, Render, Styled, Task, Window,
    div, rems,
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
    fn build_view(&self, window: &mut Window, cx: &mut App) -> Option<AnyView> {
        Some(match self {
            Self::Storefronts(None) => cx.new(Storefronts::new).into(),
            Self::Plugins(None) => cx.new(Plugins::new).into(),
            Self::Storefronts(Some(id)) | Self::Plugins(Some(id)) => {
                let component = cx.component(id)?;
                cx.new(|cx| Config::new(component, window, cx)).into()
            }
            Self::Appearance => cx.new(|_| Appearance).into(),
        })
    }
}

fn settings_item(page: SettingsPage) -> SidebarItem<SettingsPage> {
    let label = page.label();
    nav_item(page).label(label)
}

fn status_label(status: &Status) -> Label {
    match status {
        Status::Initializing => Label::new("INITIALIZING").variant_secondary(),
        Status::Active => Label::new("ACTIVE").variant_accent(),
        Status::Inactive(InactiveReason::Disabled) => Label::new("DISABLED"),
        Status::Inactive(InactiveReason::Unauthenticated) => {
            Label::new("UNAUTHENTICATED").variant_warning()
        }
        Status::Inactive(InactiveReason::Error(_)) => Label::new("ERROR").variant_danger(),
    }
}

pub struct Settings {
    current_page: SettingsPage,
    active_view: Option<AnyView>,
    _components_task: Task<()>,
}

impl Route for Settings {
    type Args = SettingsPage;
    type State = ();

    fn build(page: SettingsPage, _state: (), window: &mut Window, cx: &mut Context<Self>) -> Self {
        let active_view = page.build_view(window, cx);

        cx.observe_global_in::<Navigator>(window, |this, window, cx| {
            if let Page::Settings(Some(page)) = cx.navigator().current_page()
                && page != this.current_page
            {
                this.current_page = page.clone();
                this.active_view = page.build_view(window, cx);
                cx.notify();
            }
        })
        .detach();

        Self {
            current_page: page,
            active_view,
            _components_task: cx.listen_any_in(
                window,
                &[Topic::ComponentRegistered, Topic::ComponentState],
                Self::build_missing_view,
            ),
        }
    }
}

impl Settings {
    fn build_missing_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.active_view.is_none() {
            self.active_view = self.current_page.build_view(window, cx);
            cx.notify();
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
            .children(self.active_view.clone())
    }
}
