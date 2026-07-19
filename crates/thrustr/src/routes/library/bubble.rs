use super::GridDims;
use domain::game::SectionIndex;
use gpui::{
    App, Entity, FontWeight, IntoElement, ParentElement, Pixels, RenderOnce, SharedString, Size,
    Styled, UniformListScrollHandle, Window, div, px,
};
use theme::ThemeExt;
use ui::{SCROLLBAR_WIDTH, ScrollbarAxis, ScrollbarState};

const BUBBLE_HEIGHT: Pixels = px(28.);
/// Gap between the bubble and the scrollbar track.
const BUBBLE_GAP: Pixels = px(8.);

/// Builds the index bubble for the current scroll position, if the user is
/// scrolling and the section it lands in is known.
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

    let list = scroll_handle.0.borrow();
    let viewport = list.base_handle.bounds().size.height;
    let offset = list.base_handle.offset().y.abs();
    let max_offset = list.base_handle.max_offset().y;
    drop(list);

    let top_item = top_visible_item(offset, max_offset, viewport, dims)?;
    let label = sections.label_for(top_item)?;
    let thumb_center = scrollbar.thumb_center(ScrollbarAxis::Vertical)?;

    Some(IndexBubble::new(
        label.to_string(),
        thumb_center,
        size.height,
        opacity,
    ))
}

/// Index of the first game on screen, given the current scroll position.
fn top_visible_item(
    offset: Pixels,
    max_offset: Pixels,
    viewport: Pixels,
    dims: &GridDims,
) -> Option<usize> {
    if dims.num_rows == 0 || dims.num_cols == 0 {
        return None;
    }

    let row_height = (max_offset + viewport) / dims.num_rows as f32;
    if row_height <= Pixels::ZERO {
        return None;
    }

    let top_row = (offset / row_height).floor() as usize;
    Some(top_row * dims.num_cols)
}

/// The pill shown beside the scrollbar while scrolling the library, labelling
/// the section currently at the top of the viewport.
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

#[cfg(test)]
mod tests {
    use super::*;

    const GAMES: usize = 3035;
    const COLS: usize = 6;
    const VIEWPORT: Pixels = px(821.);
    const MAX_OFFSET: Pixels = px(206_639.);

    fn dims() -> GridDims {
        GridDims {
            num_cols: COLS,
            num_rows: GAMES.div_ceil(COLS),
            visible_rows: 4,
        }
    }

    #[test]
    fn top_of_the_list_is_the_first_game() {
        let item = top_visible_item(px(0.), MAX_OFFSET, VIEWPORT, &dims());
        assert_eq!(item, Some(0));
    }

    #[test]
    fn bottom_of_the_list_reaches_the_last_rows() {
        let dims = dims();
        let item = top_visible_item(MAX_OFFSET, MAX_OFFSET, VIEWPORT, &dims).unwrap();

        // Two rows fit on screen, so at full scroll the topmost row still
        // intersecting the viewport is the third from the end.
        let last_row = dims.num_rows - 1;
        let top_row = item / COLS;
        assert!(
            (last_row - 3..=last_row).contains(&top_row),
            "row {top_row} should be within a row of the end ({last_row})",
        );

        assert!(item > GAMES / 2, "{item} should be past the midpoint");
    }

    #[test]
    fn midpoint_lands_near_the_middle_of_the_library() {
        let item = top_visible_item(MAX_OFFSET / 2., MAX_OFFSET, VIEWPORT, &dims()).unwrap();

        let expected = GAMES / 2;
        assert!(
            item.abs_diff(expected) < COLS * 2,
            "{item} should be within a row or two of {expected}",
        );
    }

    #[test]
    fn an_empty_library_has_no_position() {
        let empty = GridDims {
            num_cols: 0,
            num_rows: 0,
            visible_rows: 0,
        };
        assert_eq!(top_visible_item(px(0.), px(0.), VIEWPORT, &empty), None);
    }
}
