use gpui::{
    App, IntoElement, ParentElement, Refineable, RenderOnce, SharedString, StyleRefinement, Styled,
    Window, div, rems,
};
use theme::ThemeExt;

#[derive(IntoElement, Default)]
pub struct Empty {
    style: StyleRefinement,
    text: SharedString,
}

impl Empty {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            style: StyleRefinement::default(),
            text: text.into(),
        }
    }
}

impl Styled for Empty {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Empty {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();

        let mut empty = div()
            .flex()
            .items_center()
            .justify_center()
            .w_full()
            .min_w_0()
            .px(rems(1.5))
            .py(rems(3.))
            .rounded(theme.radius.lg)
            .border_1()
            .border_dashed()
            .border_color(theme.colors.tertiary.opacity(0.4))
            .text_color(theme.colors.tertiary)
            .text_size(theme.text.md)
            .child(self.text);

        empty.style().refine(&self.style);
        empty
    }
}
