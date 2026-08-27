use super::grid::{GridDims, GridMetrics};
use domain::section_index::SectionIndex;
use gpui::{
    App, Entity, FontWeight, IntoElement, ParentElement, Pixels, RenderOnce, SharedString, Size,
    Styled, UniformListScrollHandle, Window, div, px,
};
use theme::ThemeExt;
use ui::{SCROLLBAR_WIDTH, ScrollbarAxis, ScrollbarState};

const BUBBLE_HEIGHT: Pixels = px(28.);
const BUBBLE_GAP: Pixels = px(8.);

pub(super) fn index_bubble(
    scrollbar: &Entity<ScrollbarState>,
    scroll_handle: &UniformListScrollHandle,
    sections: &SectionIndex,
    dims: &GridDims,
    size: Size<Pixels>,
    cx: &App,
) -> Option<IndexBubble> {
    let scrollbar = scrollbar.read(cx);
    let opacity = scrollbar.scroll_opacity();
    if opacity <= 0. || sections.is_empty() {
        return None;
    }

    let metrics = GridMetrics::measure(scroll_handle, dims.num_rows)?;
    let top_item = metrics.first_touching_row() * dims.num_cols;
    let label = sections.label_for(top_item)?;
    let thumb_center = scrollbar.thumb_center(ScrollbarAxis::Vertical)?;

    Some(IndexBubble::new(
        label.to_string(),
        thumb_center,
        size.height,
        opacity,
    ))
}

#[derive(IntoElement)]
pub(super) struct IndexBubble {
    label: SharedString,
    top: Pixels,
    opacity: f32,
}

impl IndexBubble {
    fn new(
        label: impl Into<SharedString>,
        thumb_center: Pixels,
        track_height: Pixels,
        opacity: f32,
    ) -> Self {
        let travel = (track_height - BUBBLE_HEIGHT).max(Pixels::ZERO);
        Self {
            label: label.into(),
            top: (thumb_center - BUBBLE_HEIGHT / 2.).clamp(Pixels::ZERO, travel),
            opacity,
        }
    }
}

impl RenderOnce for IndexBubble {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();

        div()
            .absolute()
            .top(self.top)
            .right(SCROLLBAR_WIDTH + BUBBLE_GAP)
            .h(BUBBLE_HEIGHT)
            .min_w(BUBBLE_HEIGHT)
            .px(px(10.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(theme.radius.full)
            .bg(theme.colors.accent)
            .text_color(theme.colors.background)
            .font_weight(FontWeight::SEMIBOLD)
            .opacity(self.opacity)
            .child(self.label)
    }
}
