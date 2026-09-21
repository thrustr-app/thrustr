use crate::{
    globals::ComponentRegistryExt,
    routes::{self, RouteHandle},
};
use domain::game::GameId;
use gpui::{AnyView, App, AppContext, EmptyView, Global, SharedString, Window};
use std::{collections::VecDeque, mem::replace};
use ui::{Icon, Sidebar, SidebarItem};

const MAX_HISTORY: usize = 20;

pub fn init(cx: &mut App) {
    cx.set_global(Navigator::new(Page::Home));
}

/// A navigable node in the page tree.
pub trait NavNode: Into<Page> + Clone {
    fn label(&self) -> &'static str;
    fn icon(&self) -> Icon;
    fn is_parent_of(&self, other: &Self) -> bool;
}

/// Binds a sidebar to the navigator.
pub trait NavSidebar<T> {
    fn nav(self, current: T) -> Self;
}

impl<T: NavNode + PartialEq + 'static> NavSidebar<T> for Sidebar<T> {
    fn nav(self, current: T) -> Self {
        self.value(current)
            .matches(T::is_parent_of)
            .on_change(|page: &T, _, cx| cx.navigate(page.clone()))
    }
}

/// Builds a sidebar item that navigates to `page` and reflects its active state.
pub fn nav_item<T: NavNode + PartialEq + 'static>(page: T) -> SidebarItem<T> {
    SidebarItem::new(page.label()).icon(page.icon()).value(page)
}

/// Determines whether two pages reuse the same root view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Section {
    Home,
    Library,
    Collections,
    Game(GameId),
    Settings,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Page {
    Home,
    Library,
    Collections,
    Game(GameId),
    Settings(Option<SettingsPage>),
}

impl NavNode for Page {
    fn label(&self) -> &'static str {
        match self {
            Self::Home => "Home",
            Self::Library => "Library",
            Self::Collections => "Collections",
            Self::Game(_) => "",
            Self::Settings(None) => "Settings",
            Self::Settings(Some(sub)) => sub.label(),
        }
    }

    fn icon(&self) -> Icon {
        match self {
            Self::Home => Icon::home(),
            Self::Library => Icon::library(),
            Self::Collections => Icon::collections(),
            Self::Game(_) => Icon::library(),
            Self::Settings(_) => Icon::settings(),
        }
    }

    fn is_parent_of(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Library, Self::Game(_)) => true,
            (Self::Settings(None), Self::Settings(_)) => true,
            (Self::Settings(Some(a)), Self::Settings(Some(b))) => a.is_parent_of(b),
            _ => self == other,
        }
    }
}

impl Page {
    pub fn build_view(&self, window: &mut Window, cx: &mut App) -> Box<dyn RouteHandle> {
        match self {
            Self::Home => Box::new(cx.new(|_| routes::Home)),
            Self::Library => Box::new(cx.new(|cx| routes::Library::new(window, cx))),
            Self::Collections => Box::new(cx.new(|_| routes::Collections)),
            Self::Game(id) => Box::new(cx.new(|cx| routes::Game::new(*id, cx))),
            Self::Settings(Some(sub)) => {
                Box::new(cx.new(|cx| routes::Settings::new(sub.clone(), cx)))
            }
            _ => Box::new(cx.new(|_| EmptyView)),
        }
    }

    pub(crate) fn section(&self) -> Section {
        match self {
            Self::Home => Section::Home,
            Self::Library => Section::Library,
            Self::Collections => Section::Collections,
            Self::Game(id) => Section::Game(*id),
            Self::Settings(_) => Section::Settings,
        }
    }

    fn resolve(self) -> Self {
        match self {
            Self::Settings(None) => SettingsPage::Storefronts(None).into(),
            other => other,
        }
    }
}

impl From<SettingsPage> for Page {
    fn from(sub: SettingsPage) -> Self {
        Self::Settings(Some(sub))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsPage {
    Storefronts(Option<SharedString>),
    Plugins(Option<SharedString>),
    Appearance,
}

impl Default for SettingsPage {
    fn default() -> Self {
        Self::Storefronts(None)
    }
}

impl NavNode for SettingsPage {
    fn label(&self) -> &'static str {
        match self {
            Self::Storefronts(_) => "Storefronts",
            Self::Plugins(_) => "Plugins",
            Self::Appearance => "Appearance",
        }
    }

    fn icon(&self) -> Icon {
        match self {
            Self::Storefronts(_) => Icon::storefront(),
            Self::Plugins(_) => Icon::plugin(),
            Self::Appearance => Icon::appearance(),
        }
    }

    fn is_parent_of(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Storefronts(None), Self::Storefronts(_)) => true,
            (Self::Plugins(None), Self::Plugins(_)) => true,
            _ => self == other,
        }
    }
}

impl SettingsPage {
    pub fn build_view(&self, cx: &mut App) -> AnyView {
        match self {
            Self::Storefronts(None) => cx.new(routes::Storefronts::new).into(),
            Self::Plugins(None) => cx.new(routes::Plugins::new).into(),
            Self::Storefronts(Some(id)) | Self::Plugins(Some(id)) => match cx.component(id) {
                Some(component) => cx.new(|cx| routes::Config::new(cx, component)).into(),
                None => cx.new(|_| EmptyView).into(),
            },
            Self::Appearance => cx.new(|_| routes::Appearance).into(),
        }
    }
}

#[derive(Debug)]
pub struct Navigator {
    current: Page,
    history: VecDeque<Page>,
}

impl Global for Navigator {}

impl Navigator {
    fn new(initial: Page) -> Self {
        Self {
            current: initial,
            history: VecDeque::new(),
        }
    }

    pub fn current_page(&self) -> Page {
        self.current.clone()
    }

    fn navigate(&mut self, next: Page) {
        if self.current.is_parent_of(&next) || next.is_parent_of(&self.current) {
            self.current = next;
            return;
        }

        let previous = replace(&mut self.current, next);
        self.history.push_back(previous);

        if self.history.len() > MAX_HISTORY {
            self.history.pop_front();
        }
    }

    fn navigate_back(&mut self) {
        if let Some(previous) = self.history.pop_back() {
            self.current = previous;
        }
    }
}

/// Extension trait that provides navigation-related methods.
pub trait NavigatorExt {
    /// Returns a reference to the navigator.
    fn navigator(&self) -> &Navigator;
    /// Navigates to the given page, pushing the current page onto the history.
    fn navigate(&mut self, page: impl Into<Page>);
    /// Navigates back to the previous page, if available.
    fn navigate_back(&mut self);
}

impl NavigatorExt for App {
    fn navigator(&self) -> &Navigator {
        self.global::<Navigator>()
    }

    fn navigate(&mut self, page: impl Into<Page>) {
        let next = page.into().resolve();

        if self.global::<Navigator>().current == next {
            return;
        }

        self.global_mut::<Navigator>().navigate(next);
    }

    fn navigate_back(&mut self) {
        if self.global::<Navigator>().history.is_empty() {
            return;
        }

        self.global_mut::<Navigator>().navigate_back();
    }
}
