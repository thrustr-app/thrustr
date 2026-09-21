use super::{CACHE_OVERSCAN_ROWS, CARD_GAP, CARD_MIN_WIDTH};
use gpui::{Pixels, UniformListScrollHandle};
use std::num::NonZeroUsize;

pub(super) struct GridDims {
    pub(super) num_cols: usize,
    pub(super) num_rows: usize,
    pub(super) visible_rows: usize,
    pub(super) card_width: Pixels,
}

impl GridDims {
    pub(super) fn compute(
        grid_width: Pixels,
        grid_height: Pixels,
        game_count: usize,
        content_height: Option<Pixels>,
    ) -> Self {
        let num_cols = ((grid_width + CARD_GAP) / (CARD_MIN_WIDTH + CARD_GAP)).floor() as usize;
        let num_cols = num_cols.max(1);
        let num_rows = game_count.div_ceil(num_cols);
        let card_width = (grid_width - CARD_GAP * (num_cols - 1) as f32) / num_cols as f32;

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
            card_width,
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

    #[track_caller]
    fn check_top_item(offset: Pixels, expected: usize) {
        assert_eq!(top_item(offset), expected);
    }

    #[track_caller]
    fn check_top_item_near(offset: Pixels, expected: usize, tolerance: usize) {
        let item = top_item(offset);
        assert!(
            item.abs_diff(expected) <= tolerance,
            "{item} should be within {tolerance} of {expected}",
        );
    }

    #[track_caller]
    fn check_no_metrics(num_rows: usize) {
        let handle = UniformListScrollHandle::new();
        assert!(GridMetrics::measure(&handle, num_rows).is_none());
    }

    #[track_caller]
    fn check_touching_and_nearest(
        offset: Pixels,
        expected_touching: usize,
        expected_nearest: usize,
    ) {
        let metrics = metrics(offset);
        assert_eq!(metrics.first_touching_row(), expected_touching);
        assert_eq!(metrics.nearest_row(), expected_nearest);
    }

    #[track_caller]
    fn check_row_is_visible(offset: Pixels, row: usize, expected: bool) {
        assert_eq!(
            metrics(offset).row_is_visible(row),
            expected,
            "row {row} at offset {offset:?} should{} be visible",
            if expected { "" } else { " not" },
        );
    }

    #[test]
    fn top_of_the_list_is_the_first_game() {
        check_top_item(px(0.), 0);
    }

    #[test]
    fn bottom_of_the_list_reaches_the_last_rows() {
        let last_row = num_rows() - 1;
        check_top_item_near(MAX_OFFSET, last_row * COLS, COLS * 3);
    }

    #[test]
    fn midpoint_lands_near_the_middle_of_the_library() {
        check_top_item_near(MAX_OFFSET / 2., GAMES / 2, COLS * 2);
    }

    #[test]
    fn an_unmeasured_list_has_no_metrics() {
        check_no_metrics(num_rows());
        check_no_metrics(0);
    }

    #[test]
    fn a_partially_scrolled_row_is_still_the_one_touched() {
        check_touching_and_nearest(row_height() * 3.6, 3, 4);
    }

    #[test]
    fn a_partial_rows_counts_as_visible() {
        let offset = row_height() * 3.99;

        check_row_is_visible(offset, 3, true);
        check_row_is_visible(offset, 4, true);
        check_row_is_visible(offset, 2, false);
    }

    const FOUR_COL_WIDTH: Pixels = px(744.);

    #[track_caller]
    fn check_num_cols(grid_width: Pixels, expected: usize) {
        let dims = GridDims::compute(grid_width, px(1000.), 100, None);
        assert_eq!(dims.num_cols, expected);
    }

    #[test]
    fn narrower_grids_fit_fewer_columns() {
        check_num_cols(CARD_MIN_WIDTH, 1);
        check_num_cols(CARD_MIN_WIDTH * 2. + CARD_GAP, 2);
        check_num_cols(px(10.), 1);
    }

    #[track_caller]
    fn check_num_rows(game_count: usize, expected: usize) {
        let dims = GridDims::compute(FOUR_COL_WIDTH, px(1000.), game_count, None);
        assert_eq!(dims.num_rows, expected);
    }

    #[test]
    fn rows_round_up_to_fit_all_games() {
        check_num_rows(0, 0);
        check_num_rows(4, 1);
        check_num_rows(5, 2);
        check_num_rows(8, 2);
    }

    #[test]
    fn cards_fill_the_row_width_exactly() {
        let dims = GridDims::compute(FOUR_COL_WIDTH, px(1000.), 10, None);

        let total = dims.card_width * dims.num_cols as f32 + CARD_GAP * (dims.num_cols - 1) as f32;
        assert_eq!(total, FOUR_COL_WIDTH);
    }

    #[test]
    fn cache_capacity_is_overscan_before_layout() {
        let dims = GridDims::compute(FOUR_COL_WIDTH, px(500.), 40, None);

        assert_eq!(dims.visible_rows, 0);
        assert_eq!(dims.cache_capacity(), dims.num_cols * CACHE_OVERSCAN_ROWS);
    }

    #[test]
    fn cache_capacity_grows_with_viewport() {
        // 10 rows measuring 1000px of content means each row is 100px tall.
        // A 350px-tall viewport should show 4 of them, rounded up.
        let dims = GridDims::compute(FOUR_COL_WIDTH, px(350.), 40, Some(px(1000.)));

        assert_eq!(dims.visible_rows, 4);
        assert_eq!(
            dims.cache_capacity(),
            dims.num_cols * (4 + CACHE_OVERSCAN_ROWS)
        );
    }
}
