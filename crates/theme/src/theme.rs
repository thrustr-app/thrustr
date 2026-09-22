use gpui::{AbsoluteLength, Hsla};
use serde::Deserialize;
use std::ops::Deref;
use std::sync::Arc;

define_theme_group!(SidebarColors: Hsla {
    background,
    primary,
    secondary,
    border,
    surface,
    hover,
    logo,
});

define_theme_group!(TitlebarColors: Hsla {
    background,
    primary,
    secondary,
    border,
    hover,
});

define_theme_colors!(
    colors: [
        background,
        primary,
        secondary,
        secondary_background,
        secondary_foreground,
        tertiary,
        surface,
        surface_sunken,
        hover,
        accent,
        accent_background,
        accent_foreground,
        border,
        warning,
        warning_background,
        warning_foreground,
        danger,
        danger_background,
        danger_foreground,
        overlay,
    ],
    groups: [
        sidebar: SidebarColors,
        titlebar: TitlebarColors,
    ]
);

define_theme_group!(ThemeRadius: AbsoluteLength {
    sm,
    md,
    lg,
    pill,
});

define_theme_group!(ThemeText: AbsoluteLength {
    sm,
    md,
    lg,
    xl,
});

#[doc(hidden)]
#[derive(Debug, Deserialize)]
pub struct ThemeData {
    pub manifest: ThemeManifest,
    pub colors: ThemeColors,
    pub radius: ThemeRadius,
    pub text: ThemeText,
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
    pub text: Option<PartialThemeText>,
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
            text: self
                .text
                .take()
                .map(|t| t.merge(&other.text))
                .unwrap_or_else(|| other.text.clone()),
        })
    }
}
