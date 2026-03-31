use gpui::{
    App, FontWeight, IntoElement, ParentElement, RenderOnce, SharedString, Styled, Window, div,
    rems,
};
use theme_manager::ThemeExt;

#[derive(IntoElement)]
pub struct Topbar {
    title: SharedString,
}

impl Topbar {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
        }
    }
}

impl RenderOnce for Topbar {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();

        div()
            .px(rems(2.))
            .h(rems(6.))
            .bg(theme.colors.background)
            .w_full()
            .flex_shrink_0()
            .flex()
            .items_center()
            .child(
                div()
                    .child(self.title)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(rems(1.5))
                    .text_color(theme.colors.primary),
            )
    }
}
