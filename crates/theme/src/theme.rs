use gpui::{AbsoluteLength, Hsla};
use serde::Deserialize;

define_theme_colors!(
    background,
    primary,
    secondary,
    surface,
    accent,
    border,
    error,
    warning,
    overlay,
    sidebar_background,
    sidebar_primary,
    sidebar_secondary,
    sidebar_surface,
    sidebar_logo,
    card_background,
    card_surface,
    card_primary,
    card_secondary,
);

define_theme_radius!(sm, lg, full);

#[derive(Debug, Deserialize)]
pub struct Theme {
    pub manifest: ThemeManifest,
    pub colors: ThemeColors,
    pub radius: ThemeRadius,
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
    pub radius: Option<PartialThemeRadius>,
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
            radius: self
                .radius
                .take()
                .map(|r| r.merge(&other.radius))
                .unwrap_or_else(|| other.radius.clone()),
        }
    }
}
