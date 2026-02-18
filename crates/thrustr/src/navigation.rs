use gpui_router::{Location, NavLink};

/// Shared behaviour for all route enums.
pub trait Route: Copy {
    /// Returns `(label, path)`.
    fn route(&self) -> (&'static str, &'static str);

    fn as_str(&self) -> &'static str {
        self.route().0
    }

    fn as_path(&self) -> &'static str {
        self.route().1
    }

    fn nav_link(&self) -> NavLink {
        NavLink::new().to(self.as_path())
    }
}

#[derive(Debug, Clone, Copy)]
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
            (Settings(_), Settings(_)) => true, // ignore inner SettingsPage
            _ => false,
        }
    }
}

impl Eq for Page {}

impl Route for Page {
    fn route(&self) -> (&'static str, &'static str) {
        match self {
            Page::Home => ("home", "/"),
            Page::Library => ("library", "/library"),
            Page::Collections => ("collections", "/collections"),
            Page::Settings(sub) => sub.route(),
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
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum SettingsPage {
    #[default]
    Plugins,
    Appearance,
}

impl Route for SettingsPage {
    fn route(&self) -> (&'static str, &'static str) {
        match self {
            SettingsPage::Plugins => ("plugins", "/settings/plugins"),
            SettingsPage::Appearance => ("appearance", "/settings/appearance"),
        }
    }
}

impl SettingsPage {
    fn from_path(path: &str) -> Self {
        match path {
            "/settings/appearance" => Self::Appearance,
            "/settings/plugins" => Self::Plugins,
            _ => Self::default(),
        }
    }

    pub fn icon_path(&self) -> &'static str {
        match self {
            SettingsPage::Plugins => "icons/plugins.svg",
            SettingsPage::Appearance => "icons/appearance.svg",
        }
    }

    pub fn as_str_pretty(&self) -> &'static str {
        match self {
            SettingsPage::Plugins => "Plugins",
            SettingsPage::Appearance => "Appearance",
        }
    }
}

pub trait LocationExt {
    fn page(&self) -> Page;
    fn settings_page(&self) -> Option<SettingsPage> {
        if let Page::Settings(settings_page) = self.page() {
            Some(settings_page)
        } else {
            None
        }
    }
}

impl LocationExt for Location {
    fn page(&self) -> Page {
        let path = self.pathname.as_str();
        match path {
            "/" => Page::Home,
            p if p.starts_with("/library") => Page::Library,
            p if p.starts_with("/collections") => Page::Collections,
            p if p.starts_with("/settings") => Page::Settings(SettingsPage::from_path(p)),
            _ => Page::Home,
        }
    }
}
