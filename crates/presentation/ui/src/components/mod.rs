mod alert;
mod button;
mod card;
mod dialog;
mod input;
mod label;

pub use alert::*;
pub use button::*;
pub use card::*;
pub use dialog::*;
use gpui::App;
pub use input::*;
pub use label::*;

pub fn init(cx: &mut App) {
    dialog::init(cx);
    input::init(cx);
}
