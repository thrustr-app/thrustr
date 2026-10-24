use super::{CACHE_OVERSCAN_ROWS, CARD_GAP, CARD_MIN_WIDTH};
use gpui::{Pixels, UniformListScrollHandle};

#[derive(Clone, Copy)]
pub(super) struct GridLayout {
    pub(super) num_cols: usize,
    pub(super) num_rows: usize,
    pub(super) card_width: Pixels,
}

impl GridLayout {
    pub(super) fn compute(grid_width: Pixels, game_count: usize) -> Self {
        let num_cols = ((grid_width + CARD_GAP) / (CARD_MIN_WIDTH + CARD_GAP)).floor() as usize;
        let num_cols = num_cols.max(1);

        Self {
            num_cols,
            num_rows: game_count.div_ceil(num_cols),
            card_width: (grid_width - CARD_GAP * (num_cols - 1) as f32) / num_cols as f32,
        }
    }

    pub(super) fn cache_capacity(&self, visible_rows: usize) -> usize {
        self.num_cols * (visible_rows + CACHE_OVERSCAN_ROWS)
    }
}

#[derive(Clone, Copy)]
pub(super) struct GridViewport {
    row_height: Pixels,
    offset: Pixels,
    height: Pixels,
    num_rows: usize,
}

impl GridViewport {
    pub(super) fn measure(
        scroll_handle: &UniformListScrollHandle,
        num_rows: usize,
    ) -> Option<Self> {
        if num_rows == 0 {
            return None;
        }

        let list = scroll_handle.0.borrow();
        let row_height = list.last_item_size?.contents.height / num_rows as f32;

        (row_height > Pixels::ZERO).then(|| Self {
            row_height,
            offset: -list.base_handle.offset().y,
            height: list.base_handle.bounds().size.height,
            num_rows,
        })
    }

    pub(super) fn first_visible_row(&self) -> usize {
        self.scrolled_rows().floor() as usize
    }

    pub(super) fn row_nearest_top(&self) -> usize {
        self.scrolled_rows().round() as usize
    }

    pub(super) fn visible_rows(&self) -> usize {
        (self.height / self.row_height).ceil() as usize
    }

    pub(super) fn is_row_visible(&self, row: usize) -> bool {
        let top = self.row_top(row);
        top < self.offset + self.height && top + self.row_height > self.offset
    }

    pub(super) fn rows_from_top(&self, row: usize) -> f32 {
        row as f32 - self.scrolled_rows()
    }

    pub(super) fn scroll_delta(&self, from_row: usize, to_row: usize) -> Pixels {
        self.row_height * (to_row as f32 - from_row as f32)
    }

    /// The scroll offset that puts `row` at `rows_from_top`.
    pub(super) fn offset_to_place(&self, row: usize, rows_from_top: f32) -> Pixels {
        let rows_from_top = rows_from_top.min(self.height / self.row_height - 1.);
        let max_offset = (self.row_height * self.num_rows as f32 - self.height).max(Pixels::ZERO);
        (self.row_height * (row as f32 - rows_from_top)).clamp(Pixels::ZERO, max_offset)
    }

    pub(super) fn offset_to_reveal(&self, row: usize) -> Pixels {
        let top = self.row_top(row);
        let bottom = top + self.row_height;

        if top < self.offset {
            top
        } else if bottom > self.offset + self.height {
            bottom - self.height
        } else {
            self.offset
        }
    }

    fn scrolled_rows(&self) -> f32 {
        self.offset / self.row_height
    }

    fn row_top(&self, row: usize) -> Pixels {
        self.row_height * row as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{ItemSize, point, px, size};

    const FOUR_COL_WIDTH: Pixels = px(744.);

    #[track_caller]
    fn check_num_cols(grid_width: Pixels, expected: usize) {
        assert_eq!(GridLayout::compute(grid_width, 100).num_cols, expected);
    }

    #[test]
    fn grid_fits_as_many_columns_as_its_width_allows() {
        let two_cards = CARD_MIN_WIDTH * 2. + CARD_GAP;

        check_num_cols(CARD_MIN_WIDTH, 1);
        check_num_cols(two_cards - px(1.), 1);
        check_num_cols(two_cards, 2);
        check_num_cols(FOUR_COL_WIDTH, 4);
    }

    #[test]
    fn narrow_grid_still_has_one_column() {
        check_num_cols(px(10.), 1);
    }

    #[track_caller]
    fn check_num_rows(game_count: usize, expected: usize) {
        assert_eq!(
            GridLayout::compute(FOUR_COL_WIDTH, game_count).num_rows,
            expected
        );
    }

    #[test]
    fn rows_fit_every_game() {
        check_num_rows(0, 0);
        check_num_rows(1, 1);
        check_num_rows(4, 1);
        check_num_rows(5, 2);
        check_num_rows(8, 2);
        check_num_rows(9, 3);
    }

    #[test]
    fn cards_grow_to_fill_the_row() {
        for width in (180..=2560).map(|width| px(width as f32)) {
            let layout = GridLayout::compute(width, 100);
            let cols = layout.num_cols as f32;
            let total = layout.card_width * cols + CARD_GAP * (cols - 1.);

            assert!(
                (total - width).abs() < px(0.01),
                "{total:?} should fill {width:?}"
            );
            assert!(
                layout.card_width > CARD_MIN_WIDTH - px(0.01),
                "{:?} is narrower than the minimum at {width:?}",
                layout.card_width,
            );
        }
    }

    #[test]
    fn cache_holds_visible_rows_plus_overscan() {
        let layout = GridLayout::compute(FOUR_COL_WIDTH, 40);

        assert_eq!(layout.cache_capacity(0), 4 * CACHE_OVERSCAN_ROWS);
        assert_eq!(layout.cache_capacity(5), 4 * (5 + CACHE_OVERSCAN_ROWS));
    }

    const ROW_HEIGHT: Pixels = px(100.);
    const NUM_ROWS: usize = 50;
    const VIEWPORT_ROWS: f32 = 2.5;
    const MAX_SCROLLED_ROWS: f32 = NUM_ROWS as f32 - VIEWPORT_ROWS;

    fn viewport(scrolled_rows: f32) -> GridViewport {
        GridViewport {
            row_height: ROW_HEIGHT,
            offset: ROW_HEIGHT * scrolled_rows,
            height: ROW_HEIGHT * VIEWPORT_ROWS,
            num_rows: NUM_ROWS,
        }
    }

    fn measured_list(scrolled_rows: f32) -> UniformListScrollHandle {
        let handle = UniformListScrollHandle::new();
        {
            let mut list = handle.0.borrow_mut();
            list.last_item_size = Some(ItemSize {
                contents: size(px(0.), ROW_HEIGHT * NUM_ROWS as f32),
                ..Default::default()
            });
            list.base_handle
                .set_offset(point(px(0.), -ROW_HEIGHT * scrolled_rows));
        }
        handle
    }

    #[track_caller]
    fn assert_scrolled_rows(offset: Pixels, expected: f32) {
        let rows = offset / ROW_HEIGHT;
        assert!(
            (rows - expected).abs() < 1e-3,
            "scrolled {rows} rows, expected {expected}",
        );
    }

    #[test]
    fn unmeasured_list_has_no_viewport() {
        let handle = UniformListScrollHandle::new();
        assert!(GridViewport::measure(&handle, NUM_ROWS).is_none());
    }

    #[test]
    fn empty_grid_has_no_viewport() {
        assert!(GridViewport::measure(&measured_list(0.), 0).is_none());
    }

    #[test]
    fn row_height_comes_from_list_content() {
        let viewport = GridViewport::measure(&measured_list(3.5), NUM_ROWS).unwrap();
        assert_eq!(viewport.rows_from_top(10), 6.5);
    }

    #[track_caller]
    fn check_top_rows(scrolled_rows: f32, first_visible: usize, nearest_top: usize) {
        let viewport = viewport(scrolled_rows);
        assert_eq!(
            viewport.first_visible_row(),
            first_visible,
            "first visible row"
        );
        assert_eq!(
            viewport.row_nearest_top(),
            nearest_top,
            "row nearest the top"
        );
    }

    #[test]
    fn partially_scrolled_row_is_still_first_visible() {
        check_top_rows(0., 0, 0);
        check_top_rows(3.4, 3, 3);
        check_top_rows(3.6, 3, 4);
        check_top_rows(MAX_SCROLLED_ROWS, 47, 48);
    }

    #[track_caller]
    fn check_visible_rows(scrolled_rows: f32, expected: &[usize]) {
        let viewport = viewport(scrolled_rows);
        let visible: Vec<usize> = (0..NUM_ROWS)
            .filter(|&row| viewport.is_row_visible(row))
            .collect();
        assert_eq!(visible, expected);
    }

    #[test]
    fn partially_shown_rows_are_visible() {
        check_visible_rows(0., &[0, 1, 2]);
        check_visible_rows(3.5, &[3, 4, 5]);
        check_visible_rows(3.9, &[3, 4, 5, 6]);
        check_visible_rows(MAX_SCROLLED_ROWS, &[47, 48, 49]);
    }

    #[test]
    fn visible_row_count_includes_partial_rows() {
        assert_eq!(viewport(0.).visible_rows(), 3);
    }

    #[test]
    fn scroll_follows_a_moved_anchor() {
        assert_scrolled_rows(viewport(0.).scroll_delta(3, 7), 4.);
        assert_scrolled_rows(viewport(0.).scroll_delta(7, 3), -4.);
    }

    #[track_caller]
    fn check_place(row: usize, rows_from_top: f32, expected: f32) {
        assert_scrolled_rows(viewport(0.).offset_to_place(row, rows_from_top), expected);
    }

    #[test]
    fn row_is_placed_at_given_distance_from_top() {
        check_place(10, 0., 10.);
        check_place(10, 1.5, 8.5);
    }

    #[test]
    fn row_is_restored_where_it_was() {
        let rows_from_top = viewport(10.3).rows_from_top(11);
        check_place(11, rows_from_top, 10.3);
    }

    #[test]
    fn row_placed_past_bottom_is_pulled_into_view() {
        check_place(10, 5., 8.5);
    }

    #[test]
    fn placement_stays_within_the_list() {
        check_place(1, 1.5, 0.);
        check_place(NUM_ROWS - 1, 0., MAX_SCROLLED_ROWS);
    }

    #[track_caller]
    fn check_reveal(scrolled_rows: f32, row: usize, expected: f32) {
        assert_scrolled_rows(viewport(scrolled_rows).offset_to_reveal(row), expected);
    }

    #[test]
    fn revealing_visible_row_does_not_scroll() {
        check_reveal(10.3, 11, 10.3);
    }

    #[test]
    fn revealing_row_above_scrolls_it_to_top() {
        check_reveal(10.3, 10, 10.);
        check_reveal(10.3, 2, 2.);
    }

    #[test]
    fn revealing_row_below_scrolls_it_to_bottom() {
        check_reveal(10.3, 12, 10.5);
        check_reveal(0., 20, 18.5);
    }
}
