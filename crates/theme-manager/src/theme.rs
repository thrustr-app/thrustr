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
    pub border: Hsla,
}

#[derive(Debug, Deserialize)]
pub struct PartialThemeColors {
    pub background: Option<Hsla>,
    pub sidebar_background: Option<Hsla>,
    pub border: Option<Hsla>,
}

impl PartialThemeColors {
    pub fn merge(self, other: &ThemeColors) -> ThemeColors {
        ThemeColors {
            background: self.background.unwrap_or(other.background),
            sidebar_background: self.sidebar_background.unwrap_or(other.sidebar_background),
            border: self.border.unwrap_or(other.border),
        }
    }
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
