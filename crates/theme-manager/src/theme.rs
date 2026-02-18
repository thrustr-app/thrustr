use gpui::Hsla;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Theme {
    pub manifest: ThemeManifest,
    pub colors: ThemeColors,
}

impl Theme {
    /// Shorthand to get the theme ID.
    pub fn id(&self) -> &str {
        &self.manifest.id
    }
}

#[derive(Debug, Deserialize)]
pub struct ThemeManifest {
    pub id: String,
    pub name: String,
    pub authors: Vec<String>,
    pub version: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ThemeColors {
    pub background: Hsla,
    pub sidebar_background: Hsla,
    pub sidebar_foreground_primary: Hsla,
    pub sidebar_foreground_secondary: Hsla,
    pub sidebar_highlight: Hsla,
    pub card_background: Hsla,
    pub card_foreground_primary: Hsla,
    pub border: Hsla,
    pub logo: Hsla,
}

#[derive(Debug, Deserialize)]
pub struct PartialTheme {
    pub manifest: ThemeManifest,
    pub colors: Option<PartialThemeColors>,
}

impl PartialTheme {
    pub fn merge(mut self, other: &Theme) -> Theme {
        Theme {
            manifest: self.manifest,
            colors: self
                .colors
                .take()
                .map(|c| c.merge(&other.colors))
                .unwrap_or_else(|| other.colors.clone()),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct PartialThemeColors {
    pub background: Option<Hsla>,
    pub sidebar_background: Option<Hsla>,
    pub sidebar_foreground_primary: Option<Hsla>,
    pub sidebar_foreground_secondary: Option<Hsla>,
    pub sidebar_highlight: Option<Hsla>,
    pub card_background: Option<Hsla>,
    pub card_foreground_primary: Option<Hsla>,
    pub border: Option<Hsla>,
    pub logo: Option<Hsla>,
}

impl PartialThemeColors {
    pub fn merge(self, other: &ThemeColors) -> ThemeColors {
        ThemeColors {
            background: self.background.unwrap_or(other.background),
            sidebar_background: self.sidebar_background.unwrap_or(other.sidebar_background),
            sidebar_foreground_primary: self
                .sidebar_foreground_primary
                .unwrap_or(other.sidebar_foreground_primary),
            sidebar_foreground_secondary: self
                .sidebar_foreground_secondary
                .unwrap_or(other.sidebar_foreground_secondary),
            sidebar_highlight: self.sidebar_highlight.unwrap_or(other.sidebar_highlight),
            card_background: self.card_background.unwrap_or(other.card_background),
            card_foreground_primary: self
                .card_foreground_primary
                .unwrap_or(other.card_foreground_primary),
            border: self.border.unwrap_or(other.border),
            logo: self.logo.unwrap_or(other.logo),
        }
    }
}
