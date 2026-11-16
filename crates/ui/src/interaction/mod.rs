use gpui::App;

pub mod grid;

pub fn init(cx: &mut App) {
    grid::init(cx);
}
