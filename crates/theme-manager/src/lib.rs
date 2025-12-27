use assets::Assets;
use gpui::{App, Global};
use std::collections::HashMap;

mod theme;

pub use theme::*;

pub fn init(cx: &mut App) {
    let default_theme = load_default_theme();
    let default_id = default_theme.id().to_owned();

    let mut themes = load_builtin_themes(&default_theme);

    themes.insert(default_id.clone(), default_theme);

    cx.set_global(ThemeManager {
        themes,
        default_theme: default_id.clone(),
        active_theme: default_id,
    });
}

pub struct ThemeManager {
    themes: HashMap<String, Theme>,
    active_theme: String,
    default_theme: String,
}

impl ThemeManager {
    /// Get the currently active theme or the default theme as a fallback.
    fn active_theme(&self) -> &Theme {
        self.themes
            .get(&self.active_theme)
            .or_else(|| self.themes.get(&self.default_theme))
            .expect("Default theme not found")
    }
}

impl Global for ThemeManager {}

pub trait ThemeExt {
    fn theme(&self) -> &Theme;
}

impl ThemeExt for App {
    fn theme(&self) -> &Theme {
        let manager = self.global::<ThemeManager>();
        manager.active_theme()
    }
}

fn load_default_theme() -> Theme {
    let file = Assets::get("themes/default.toml").expect("Default theme not found");
    toml::from_slice(&file.data).expect("Failed to parse default theme")
}

fn load_builtin_themes(default: &Theme) -> HashMap<String, Theme> {
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
