use gpui::Hsla;
use serde::Deserialize;

define_theme_colors!(
    background,
    sidebar_background,
    sidebar_foreground_primary,
    sidebar_foreground_secondary,
    sidebar_highlight,
    card_background,
    card_foreground_primary,
    card_foreground_secondary,
    accent,
    error,
    border,
    logo,
);

#[derive(Debug, Deserialize)]
pub struct Theme {
    pub manifest: ThemeManifest,
    pub colors: ThemeColors,
}

impl Theme {
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
