use gpui::Hsla;
use serde::Deserialize;

define_theme_colors!(
    background,
    foreground_primary,
    foreground_secondary,
    highlight,
    accent,
    border,
    sidebar_background,
    sidebar_foreground_primary,
    sidebar_foreground_secondary,
    sidebar_highlight,
    logo,
    card_background,
    card_foreground_primary,
    card_foreground_secondary,
    error,
    error_background,
    warning,
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
