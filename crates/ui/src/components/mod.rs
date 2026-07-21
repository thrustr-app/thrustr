use gpui::App;

mod alert;
mod button;
mod card;
mod dialog;
mod input;
mod label;
mod scrollbar;
mod sidebar;

pub use alert::*;
pub use button::*;
pub use card::*;
pub use dialog::*;
pub use input::*;
pub use label::*;
pub use scrollbar::*;
pub use sidebar::*;

pub fn init(cx: &mut App) {
    dialog::init(cx);
    input::init(cx);
    sidebar::init(cx);
}
