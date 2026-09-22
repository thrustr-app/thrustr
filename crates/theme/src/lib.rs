use crate::error::{Result, ThemeError};
use assets::Assets;
use gpui::{App, Global};
use std::collections::HashMap;

#[macro_use]
mod macros;
mod error;
mod theme;

pub use theme::*;

const DEFAULT_THEME_PATH: &str = "themes/default.toml";

pub fn init(cx: &mut App) {
    cx.set_global(ThemeManager::new());
}

pub struct ThemeManager {
    themes: HashMap<String, Theme>,
    active: Theme,
}

impl Default for ThemeManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ThemeManager {
    pub fn new() -> Self {
        let default_theme = Theme::new(load_default_theme_data());

        let mut themes = load_builtin_themes(&default_theme);
        themes.insert(default_theme.id().to_owned(), default_theme.clone());

        Self {
            themes,
            active: default_theme,
        }
    }

    pub fn list_themes(&self) -> Vec<&ThemeManifest> {
        self.themes.values().map(|t| &t.manifest).collect()
    }

    pub fn set_active_theme(&mut self, id: &str) -> Result<()> {
        let theme = self
            .themes
            .get(id)
            .cloned()
            .ok_or_else(|| ThemeError::NotFound(id.to_owned()))?;
        self.active = theme;
        Ok(())
    }

    pub fn active_theme(&self) -> Theme {
        self.active.clone()
    }
}

impl Global for ThemeManager {}

pub trait ThemeExt {
    fn theme_manager(&self) -> &ThemeManager;
    fn theme(&self) -> Theme {
        self.theme_manager().active_theme()
    }
}

impl ThemeExt for App {
    fn theme_manager(&self) -> &ThemeManager {
        self.global::<ThemeManager>()
    }
}

fn load_default_theme_data() -> ThemeData {
    let file =
        Assets::get(DEFAULT_THEME_PATH).expect("the default theme should always be available");
    toml::from_slice(&file.data).expect("the default theme should always be valid")
}

fn load_builtin_themes(default: &ThemeData) -> HashMap<String, Theme> {
    Assets::iter()
        .filter(|path| {
            path.starts_with("themes/") && path.ends_with(".toml") && path != DEFAULT_THEME_PATH
        })
        .filter_map(|path| {
            let data = Assets::get(&path)?;
            let partial: PartialTheme = toml::from_slice(&data.data).ok()?;
            let theme = partial.merge(default);
            Some((theme.id().to_owned(), theme))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_theme_deserializes_properly() {
        let theme = ThemeManager::default().active_theme();

        assert_eq!(theme.id(), "thrustr.dark");
        assert!(!theme.manifest.name.is_empty());
        assert!(!theme.manifest.version.is_empty());
    }
}
