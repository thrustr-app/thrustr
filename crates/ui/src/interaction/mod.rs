use gpui::App;

pub(crate) mod grid;

pub(super) fn init(cx: &mut App) {
    grid::init(cx);
}
