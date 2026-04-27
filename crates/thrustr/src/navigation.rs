use crate::routes;
use gpui::{AnyView, App, AppContext, EmptyView, Global, SharedString};
use std::{collections::VecDeque, mem::replace};

const MAX_HISTORY: usize = 20;

pub fn init(cx: &mut App) {
    cx.set_global(Navigator::new(Page::Home));
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Page {
    Home,
    Library,
    Collections,
    Settings(Option<SettingsPage>),
}

impl Page {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Home => "Home",
            Self::Library => "Library",
            Self::Collections => "Collections",
            Self::Settings(None) => "Settings",
            Self::Settings(Some(sub)) => sub.label(),
        }
    }

    pub fn icon_path(&self) -> &'static str {
        match self {
            Self::Home => "icons/home.svg",
            Self::Library => "icons/library.svg",
            Self::Collections => "icons/collections.svg",
            Self::Settings(_) => "icons/settings.svg",
        }
    }

    pub fn build_view(&self, cx: &mut App) -> AnyView {
        match self {
            Self::Home => cx.new(|_| routes::Home).into(),
            Self::Library => cx.new(|cx| routes::Library::new(cx)).into(),
            Self::Collections => cx.new(|_| routes::Collections).into(),
            Self::Settings(Some(sub)) => cx.new(|cx| routes::Settings::new(sub.clone(), cx)).into(),
            _ => cx.new(|_| EmptyView).into(),
        }
    }

    /// Whether this page is the parent of the other page, i.e. the other page is a subpage of this one.
    fn is_parent_of(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Settings(None), Self::Settings(_)) => true,

            (Self::Settings(Some(a)), Self::Settings(Some(b))) => a.is_parent_of(b),
            _ => self == other,
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

impl SettingsPage {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Storefronts(_) => "Storefronts",
            Self::Plugins(_) => "Plugins",
            Self::Appearance => "Appearance",
        }
    }

    pub fn icon_path(&self) -> &'static str {
        match self {
            Self::Storefronts(_) => "icons/storefronts.svg",
            Self::Plugins(_) => "icons/plugins.svg",
            Self::Appearance => "icons/appearance.svg",
        }
    }

    pub fn build_view(&self, cx: &mut App) -> AnyView {
        match self {
            Self::Storefronts(None) => cx.new(|cx| routes::Storefronts::new(cx)).into(),
            Self::Plugins(None) => cx.new(|cx| routes::Plugins::new(cx)).into(),
            Self::Storefronts(Some(id)) | Self::Plugins(Some(id)) => {
                cx.new(|cx| routes::Config::new(cx, id)).into()
            }
            Self::Appearance => cx.new(|_| routes::Appearance).into(),
        }
    }

    /// Whether this page is the parent of the other page, i.e. the other page is a subpage of this one.
    fn is_parent_of(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Storefronts(None), Self::Storefronts(_)) => true,
            (Self::Plugins(None), Self::Plugins(_)) => true,
            _ => self == other,
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

    /// Returns `true` if the given page is the same or a parent of the current page.
    pub fn is_active_for(&self, page: impl Into<Page>) -> bool {
        page.into().is_parent_of(&self.current)
    }

    fn navigate(&mut self, page: impl Into<Page>) {
        let mut next = page.into();

        if let Page::Settings(None) = next {
            next = SettingsPage::Storefronts(None).into();
        }

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
        self.global_mut::<Navigator>().navigate(page);
    }

    fn navigate_back(&mut self) {
        self.global_mut::<Navigator>().navigate_back();
    }
}
