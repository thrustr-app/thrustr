use super::{CACHE_OVERSCAN_ROWS, CARD_MIN_GAP, CARD_WIDTH};
use gpui::{Pixels, UniformListScrollHandle};
use std::num::NonZeroUsize;

pub(super) struct GridDims {
    pub(super) num_cols: usize,
    pub(super) num_rows: usize,
    pub(super) visible_rows: usize,
}

impl GridDims {
    pub(super) fn compute(
        grid_width: Pixels,
        grid_height: Pixels,
        game_count: usize,
        content_height: Option<Pixels>,
    ) -> Self {
        let num_cols = ((grid_width + CARD_MIN_GAP) / (CARD_WIDTH + CARD_MIN_GAP)).floor() as usize;
        let num_cols = num_cols.max(1);
        let num_rows = game_count.div_ceil(num_cols);

        let visible_rows = content_height
            .filter(|h| *h > Pixels::ZERO)
            .zip(NonZeroUsize::new(num_rows))
            .map(|(content, rows)| {
                let row_height = content / rows.get() as f32;
                (grid_height / row_height).ceil() as usize
            })
            .unwrap_or(0);

        Self {
            num_cols,
            num_rows,
            visible_rows,
        }
    }

    pub(super) fn cache_capacity(&self) -> usize {
        self.num_cols * (self.visible_rows + CACHE_OVERSCAN_ROWS)
    }
}

/// The scroll position of the grid resolved into rows.
#[derive(Clone, Copy)]
pub(super) struct GridMetrics {
    row_height: Pixels,
    offset: Pixels,
    viewport: Pixels,
}

impl GridMetrics {
    pub(super) fn measure(
        scroll_handle: &UniformListScrollHandle,
        num_rows: usize,
    ) -> Option<Self> {
        let num_rows = NonZeroUsize::new(num_rows)?;
        let list = scroll_handle.0.borrow();
        let row_height = list.last_item_size?.contents.height / num_rows.get() as f32;

        (row_height > Pixels::ZERO).then(|| Self {
            row_height,
            offset: list.base_handle.offset().y.abs(),
            viewport: list.base_handle.bounds().size.height,
        })
    }

    pub(super) fn first_touching_row(&self) -> usize {
        (self.offset / self.row_height).floor() as usize
    }

    pub(super) fn nearest_row(&self) -> usize {
        (self.offset / self.row_height).round() as usize
    }

    pub(super) fn row_is_visible(&self, row: usize) -> bool {
        let top = self.row_height * row as f32;
        top < self.offset + self.viewport && top + self.row_height > self.offset
    }

    pub(super) fn scroll_delta(&self, from_row: usize, to_row: usize) -> Pixels {
        self.row_height * (to_row as f32 - from_row as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::px;

    const GAMES: usize = 3035;
    const COLS: usize = 6;
    const VIEWPORT: Pixels = px(821.);
    const MAX_OFFSET: Pixels = px(206_639.);

    fn num_rows() -> usize {
        GAMES.div_ceil(COLS)
    }

    fn row_height() -> Pixels {
        (MAX_OFFSET + VIEWPORT) / num_rows() as f32
    }

    fn metrics(offset: Pixels) -> GridMetrics {
        GridMetrics {
            row_height: row_height(),
            offset,
            viewport: VIEWPORT,
        }
    }

    fn top_item(offset: Pixels) -> usize {
        metrics(offset).first_touching_row() * COLS
    }

    #[test]
    fn top_of_the_list_is_the_first_game() {
        assert_eq!(top_item(px(0.)), 0);
    }

    #[test]
    fn bottom_of_the_list_reaches_the_last_rows() {
        let item = top_item(MAX_OFFSET);

        // Two rows fit on screen, so the last row can never reach the top.
        let last_row = num_rows() - 1;
        let top_row = item / COLS;
        assert!(
            (last_row - 3..=last_row).contains(&top_row),
            "row {top_row} should be within a row of the end ({last_row})",
        );

        assert!(item > GAMES / 2, "{item} should be past the midpoint");
    }

    #[test]
    fn midpoint_lands_near_the_middle_of_the_library() {
        let item = top_item(MAX_OFFSET / 2.);

        let expected = GAMES / 2;
        assert!(
            item.abs_diff(expected) < COLS * 2,
            "{item} should be within a row or two of {expected}",
        );
    }

    #[test]
    fn an_unmeasured_list_has_no_metrics() {
        let handle = UniformListScrollHandle::new();
        assert!(GridMetrics::measure(&handle, num_rows()).is_none());
        assert!(GridMetrics::measure(&handle, 0).is_none());
    }

    #[test]
    fn a_row_mostly_scrolled_past_is_still_the_one_touched() {
        let metrics = metrics(row_height() * 3.6);

        assert_eq!(metrics.first_touching_row(), 3);
        assert_eq!(metrics.nearest_row(), 4);
    }

    #[test]
    fn a_sliver_of_a_row_counts_as_visible() {
        let metrics = metrics(row_height() * 3.99);

        assert!(metrics.row_is_visible(3), "the sliver above the fold");
        assert!(metrics.row_is_visible(4), "the row filling the viewport");
        assert!(!metrics.row_is_visible(2), "scrolled fully past");
    }

    #[test]
    fn scrolling_further_down_the_list_moves_the_content_up() {
        let metrics = metrics(px(0.));

        assert_eq!(metrics.scroll_delta(2, 5), row_height() * 3.);
        assert_eq!(metrics.scroll_delta(5, 2), -(row_height() * 3.));
        assert_eq!(metrics.scroll_delta(4, 4), Pixels::ZERO);
    }
}
