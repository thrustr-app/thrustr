use gpui::{AnyView, App, AppContext, BorrowAppContext, Global};

use crate::routes::*;

pub fn init(cx: &mut App) {
    cx.set_global(Navigator::new(Page::Home));
}

#[derive(Debug, Clone, Copy, Eq)]
pub enum Page {
    Home,
    Library,
    Collections,
    Settings(SettingsPage),
}

impl PartialEq for Page {
    fn eq(&self, other: &Self) -> bool {
        use Page::*;
        match (self, other) {
            (Home, Home) | (Library, Library) | (Collections, Collections) => true,
            (Settings(_), Settings(_)) => true,
            _ => false,
        }
    }
}

impl Page {
    pub fn icon_path(&self) -> &'static str {
        match self {
            Page::Home => "icons/home.svg",
            Page::Library => "icons/library.svg",
            Page::Collections => "icons/collections.svg",
            Page::Settings(_) => "icons/settings.svg",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Page::Home => "home",
            Page::Library => "library",
            Page::Collections => "collections",
            Page::Settings(sub) => sub.label(),
        }
    }

    pub fn build_view(&self, cx: &mut App) -> AnyView {
        match self {
            Page::Home => cx.new(|_cx| Home).into(),
            Page::Library => cx.new(|_cx| Library).into(),
            Page::Collections => cx.new(|_cx| Collections).into(),
            Page::Settings(sub) => cx.new(|cx| Settings::new(*sub, cx)).into(),
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum SettingsPage {
    #[default]
    Storefronts,
    Plugins,
    Appearance,
}

impl SettingsPage {
    pub fn label(&self) -> &'static str {
        match self {
            SettingsPage::Storefronts => "storefronts",
            SettingsPage::Plugins => "plugins",
            SettingsPage::Appearance => "appearance",
        }
    }

    pub fn label_pretty(&self) -> &'static str {
        match self {
            SettingsPage::Storefronts => "Storefronts",
            SettingsPage::Plugins => "Plugins",
            SettingsPage::Appearance => "Appearance",
        }
    }

    pub fn icon_path(&self) -> &'static str {
        match self {
            SettingsPage::Storefronts => "icons/storefronts.svg",
            SettingsPage::Plugins => "icons/plugins.svg",
            SettingsPage::Appearance => "icons/appearance.svg",
        }
    }

    pub fn build_view(&self, cx: &mut App) -> AnyView {
        match self {
            SettingsPage::Storefronts => cx.new(|cx| Storefronts::new(cx)).into(),
            SettingsPage::Plugins => cx.new(|cx| Plugins::new(cx)).into(),
            SettingsPage::Appearance => cx.new(|_cx| Appearance).into(),
        }
    }
}

pub struct Navigator {
    current: Page,
}

impl Global for Navigator {}

impl Navigator {
    fn new(initial: Page) -> Self {
        Self { current: initial }
    }

    pub fn current_page(&self) -> Page {
        self.current
    }

    pub fn settings_page(&self) -> Option<SettingsPage> {
        if let Page::Settings(sub) = self.current {
            Some(sub)
        } else {
            None
        }
    }
}

pub fn navigate(cx: &mut App, page: Page) {
    cx.update_global::<Navigator, _>(|nav, _cx| {
        nav.current = page;
    });
}

pub fn current_page(cx: &App) -> Page {
    cx.global::<Navigator>().current_page()
}

pub fn current_settings_page(cx: &App) -> Option<SettingsPage> {
    cx.global::<Navigator>().settings_page()
}
