use crate::Icon;
use gpui::{
    App, IntoElement, ParentElement, Refineable, RenderOnce, SharedString, StyleRefinement, Styled,
    Window, div, rems,
};
use theme::ThemeExt;

#[derive(IntoElement, Default)]
pub struct Alert {
    style: StyleRefinement,
    text: SharedString,
}

impl Alert {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            style: StyleRefinement::default(),
            text: text.into(),
        }
    }
}

impl Styled for Alert {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Alert {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();

        let mut alert = div()
            .flex()
            .items_start()
            .w_full()
            .min_w_0()
            .px(rems(1.))
            .py(rems(0.875))
            .gap(rems(0.625))
            .rounded(theme.radius.lg)
            .border_2()
            .bg(theme.colors.danger_background)
            .border_color(theme.colors.danger)
            .text_color(theme.colors.danger_foreground)
            .text_size(theme.text.md)
            .line_height(rems(1.125))
            .child(Icon::danger().color(theme.colors.danger))
            .child(div().flex_1().min_w_0().line_clamp(4).child(self.text));

        alert.style().refine(&self.style);
        alert
    }
}
