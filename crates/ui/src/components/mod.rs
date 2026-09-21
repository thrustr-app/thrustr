use gpui::App;

mod alert;
mod button;
mod dialog;
mod icon;
mod index_rail;
mod input;
mod label;
mod scrollbar;
mod sidebar;
mod title_bar;

pub use alert::*;
pub use button::*;
pub use dialog::*;
pub use icon::*;
pub use index_rail::*;
pub use input::*;
pub use label::*;
pub use scrollbar::*;
pub use sidebar::{Sidebar, SidebarItem};
pub use title_bar::*;

pub fn init(cx: &mut App) {
    dialog::init(cx);
    input::init(cx);
    sidebar::init(cx);
}
