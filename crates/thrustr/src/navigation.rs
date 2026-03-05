use crate::routes;
use gpui::{AnyView, App, AppContext, BorrowAppContext, Global, SharedString};

pub fn init(cx: &mut App) {
    cx.set_global(Navigator::new(Page::Home));
}

#[derive(Debug, Clone, Eq)]
pub enum Page {
    Home,
    Library,
    Collections,
    Settings(SettingsPage),
}

impl PartialEq for Page {
    fn eq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (Page::Home, Page::Home)
                | (Page::Library, Page::Library)
                | (Page::Collections, Page::Collections)
                | (Page::Settings(_), Page::Settings(_))
        )
    }
}

impl Page {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Home => "Home",
            Self::Library => "Library",
            Self::Collections => "Collections",
            Self::Settings(sub) => sub.label(),
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
            Self::Library => cx.new(|_| routes::Library).into(),
            Self::Collections => cx.new(|_| routes::Collections).into(),
            Self::Settings(sub) => cx.new(|cx| routes::Settings::new(sub.clone(), cx)).into(),
        }
    }

    fn is_same_page(&self, other: &Self) -> bool {
        match (self, other) {
            (Page::Settings(a), Page::Settings(b)) => {
                std::mem::discriminant(a) == std::mem::discriminant(b)
            }
            _ => self == other,
        }
    }
}

#[derive(Debug, Clone, Eq)]
pub enum SettingsPage {
    Storefronts(Option<SharedString>),
    Plugins(Option<SharedString>),
    Appearance,
}

impl PartialEq for SettingsPage {
    fn eq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (SettingsPage::Storefronts(_), SettingsPage::Storefronts(_))
                | (SettingsPage::Plugins(_), SettingsPage::Plugins(_))
                | (SettingsPage::Appearance, SettingsPage::Appearance)
        )
    }
}

impl PartialEq<Page> for SettingsPage {
    fn eq(&self, other: &Page) -> bool {
        matches!(
            (self, other),
            (
                SettingsPage::Storefronts(_),
                Page::Settings(SettingsPage::Storefronts(_))
            ) | (
                SettingsPage::Plugins(_),
                Page::Settings(SettingsPage::Plugins(_))
            ) | (
                SettingsPage::Appearance,
                Page::Settings(SettingsPage::Appearance)
            )
        )
    }
}

impl PartialEq<SettingsPage> for Page {
    fn eq(&self, other: &SettingsPage) -> bool {
        other == self
    }
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
}

impl Into<Page> for SettingsPage {
    fn into(self) -> Page {
        Page::Settings(self)
    }
}

#[derive(Debug)]
pub struct Navigator {
    current: Page,
    history: Vec<Page>,
}

impl Navigator {
    fn new(initial: Page) -> Self {
        Self {
            current: initial,
            history: Vec::new(),
        }
    }
}

impl Global for Navigator {}

pub trait NavigationExt {
    fn navigate(&mut self, page: impl Into<Page>);
    fn current_page(&self) -> Page;
    fn navigate_back(&mut self);
}

impl NavigationExt for App {
    fn navigate(&mut self, page: impl Into<Page>) {
        self.update_global::<Navigator, _>(|nav, _| {
            let previous = nav.current.clone();
            nav.current = page.into();

            if !nav.current.is_same_page(&previous) {
                nav.history.push(previous);
                if nav.history.len() > 20 {
                    nav.history.remove(0);
                }
            }
        });
    }

    fn current_page(&self) -> Page {
        self.global::<Navigator>().current.clone()
    }

    fn navigate_back(&mut self) {
        self.update_global::<Navigator, _>(|nav, _| {
            if let Some(previous) = nav.history.pop() {
                nav.current = previous;
            }
        });
    }
}
