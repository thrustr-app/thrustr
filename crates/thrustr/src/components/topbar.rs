use gpui::{App, IntoElement, RenderOnce, Styled, Window, div, rems};
use theme_manager::ThemeExt;

#[derive(IntoElement)]
pub struct Topbar;

impl Topbar {
    pub fn new() -> Self {
        Self
    }
}

impl RenderOnce for Topbar {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();

        div()
            .h(rems(5.5))
            .bg(theme.colors.background)
            .w_full()
            .flex_shrink_0()
    }
}
