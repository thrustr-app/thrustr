use gpui::{AbsoluteLength, Hsla};
use serde::Deserialize;
use std::ops::Deref;
use std::sync::Arc;

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

define_theme_radius!(sm, md, lg, full);

#[doc(hidden)]
#[derive(Debug, Deserialize)]
pub struct ThemeData {
    pub manifest: ThemeManifest,
    pub colors: ThemeColors,
    pub radius: ThemeRadius,
}

#[derive(Debug, Clone)]
pub struct Theme(Arc<ThemeData>);

impl Theme {
    pub fn new(data: ThemeData) -> Self {
        Self(Arc::new(data))
    }

    pub fn id(&self) -> &str {
        &self.manifest.id
    }
}

impl Deref for Theme {
    type Target = ThemeData;

    fn deref(&self) -> &ThemeData {
        &self.0
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
    pub fn merge(mut self, other: &ThemeData) -> Theme {
        Theme::new(ThemeData {
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
        })
    }
}
