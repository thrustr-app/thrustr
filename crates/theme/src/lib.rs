use crate::error::{Result, ThemeError};
use assets::Assets;
use gpui::{App, Global};
use std::collections::HashMap;

#[macro_use]
mod macros;
mod error;
mod theme;

pub use theme::*;

pub fn init(cx: &mut App) {
    cx.set_global(ThemeManager::new());
}

pub struct ThemeManager {
    themes: HashMap<String, Theme>,
    active_theme: String,
    default_theme: String,
}

impl Default for ThemeManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ThemeManager {
    pub fn new() -> Self {
        let default_data = load_default_theme_data();
        let default_id = default_data.manifest.id.clone();

        let mut themes = load_builtin_themes(&default_data);
        themes.insert(default_id.clone(), Theme::new(default_data));

        Self {
            themes,
            default_theme: default_id.clone(),
            active_theme: default_id,
        }
    }

    pub fn list_themes(&self) -> Vec<&ThemeManifest> {
        self.themes.values().map(|t| &t.manifest).collect()
    }

    pub fn set_active_theme(&mut self, id: String) -> Result<()> {
        if self.themes.contains_key(&id) {
            self.active_theme = id;
            Ok(())
        } else {
            Err(ThemeError::NotFound(id))
        }
    }

    pub fn active_theme(&self) -> Theme {
        self.themes
            .get(&self.active_theme)
            .or_else(|| self.themes.get(&self.default_theme))
            .expect("the default theme should always be available")
            .clone()
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
        Assets::get("themes/default.toml").expect("the default theme should always be available");
    toml::from_slice(&file.data).expect("the default theme should always be valid")
}

fn load_builtin_themes(default: &ThemeData) -> HashMap<String, Theme> {
    Assets::iter()
        .filter(|path| {
            path.starts_with("themes/") && path.ends_with(".toml") && !path.contains("default")
        })
        .filter_map(|path| {
            let data = Assets::get(&path)?;
            let partial: PartialTheme = toml::from_slice(&data.data).ok()?;
            let theme = partial.merge(default);
            Some((theme.id().to_owned(), theme))
        })
        .collect()
}
