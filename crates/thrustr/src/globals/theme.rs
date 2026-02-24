use gpui::App;
use theme_manager::ThemeManager;

pub(super) fn init(cx: &mut App) {
    cx.set_global(ThemeManager::new());
}
