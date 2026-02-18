mod home;
mod library;
mod settings;

use crate::components::{Sidebar, Topbar};
use gpui::{App, IntoElement, ParentElement, RenderOnce, Styled, Window, div};
use gpui_router::{IntoLayout, Outlet};

pub use home::*;
pub use library::*;
pub use settings::*;

#[derive(IntoElement, IntoLayout)]
pub struct MainLayout {
    outlet: Outlet,
}

impl MainLayout {
    pub fn new() -> Self {
        Self {
            outlet: Outlet::new(),
        }
    }
}

impl RenderOnce for MainLayout {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div().flex().size_full().child(Sidebar::new()).child(
            div()
                .flex()
                .flex_col()
                .flex_grow()
                .child(Topbar::new())
                .child(self.outlet),
        )
    }
}
