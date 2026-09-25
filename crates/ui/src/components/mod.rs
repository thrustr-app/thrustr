use gpui::App;

mod alert;
mod button;
mod dialog;
mod empty;
mod icon;
mod input;
mod label;
mod scrollbar;
mod scrubber;
mod sidebar;
mod title_bar;
mod tooltip;

pub use alert::*;
pub use button::*;
pub use dialog::*;
pub use empty::*;
pub use icon::*;
pub use input::*;
pub use label::*;
pub use scrollbar::*;
pub use scrubber::*;
pub use sidebar::{Sidebar, SidebarItem};
pub use title_bar::*;
pub use tooltip::*;

pub fn init(cx: &mut App) {
    button::init(cx);
    dialog::init(cx);
    input::init(cx);
    sidebar::init(cx);
}
