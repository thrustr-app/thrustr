use gpui::{App, KeyBinding, actions};

pub const GRID_CONTEXT: &str = "grid";

actions!(
    grid,
    [SelectLeft, SelectRight, SelectUp, SelectDown, Activate]
);

pub(super) fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("left", SelectLeft, Some(GRID_CONTEXT)),
        KeyBinding::new("right", SelectRight, Some(GRID_CONTEXT)),
        KeyBinding::new("up", SelectUp, Some(GRID_CONTEXT)),
        KeyBinding::new("down", SelectDown, Some(GRID_CONTEXT)),
        KeyBinding::new("enter", Activate, Some(GRID_CONTEXT)),
        KeyBinding::new("space", Activate, Some(GRID_CONTEXT)),
    ]);
}

/// Direction of a grid navigation step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridDir {
    Left,
    Right,
    Up,
    Down,
}

/// Resolve the next selected index for a 2D grid laid out row-major with `cols`
/// columns over `count` items.
pub fn grid_step(
    selected: Option<usize>,
    dir: GridDir,
    count: usize,
    cols: usize,
) -> Option<usize> {
    if count == 0 {
        return None;
    }
    let cols = cols.max(1);

    let Some(current) = selected else {
        return Some(0);
    };
    let current = current.min(count - 1);

    let next = match dir {
        GridDir::Right => (current + 1).min(count - 1),
        GridDir::Left => current.saturating_sub(1),
        GridDir::Down => {
            let down = current + cols;
            if down < count { down } else { count - 1 }
        }
        GridDir::Up => current.saturating_sub(cols),
    };

    Some(next)
}

#[cfg(test)]
mod tests {
    use super::*;

    // 7 items, 3 columns:
    // 0 1 2
    // 3 4 5
    // 6
    const COUNT: usize = 7;
    const COLS: usize = 3;

    fn step(selected: usize, dir: GridDir) -> usize {
        grid_step(Some(selected), dir, COUNT, COLS).unwrap()
    }

    #[test]
    fn empty_grid_has_no_selection() {
        assert_eq!(grid_step(None, GridDir::Right, 0, COLS), None);
        assert_eq!(grid_step(Some(0), GridDir::Down, 0, COLS), None);
    }

    #[test]
    fn first_step_selects_first_item() {
        assert_eq!(grid_step(None, GridDir::Right, COUNT, COLS), Some(0));
        assert_eq!(grid_step(None, GridDir::Up, COUNT, COLS), Some(0));
    }

    #[test]
    fn horizontal_crosses_rows_and_clamps() {
        assert_eq!(step(2, GridDir::Right), 3); // end of row -> next row start
        assert_eq!(step(3, GridDir::Left), 2); // start of row -> prev row end
        assert_eq!(step(6, GridDir::Right), 6); // last item -> stays
        assert_eq!(step(0, GridDir::Left), 0); // first item -> stays
    }

    #[test]
    fn vertical_clamps_within_bounds() {
        assert_eq!(step(0, GridDir::Down), 3);
        assert_eq!(step(3, GridDir::Down), 6);
        assert_eq!(step(4, GridDir::Down), 6); // partial last row -> last item
        assert_eq!(step(6, GridDir::Down), 6); // already last -> stays
        assert_eq!(step(3, GridDir::Up), 0);
        assert_eq!(step(0, GridDir::Up), 0); // first row -> stays
    }

    #[test]
    fn stale_selection_is_clamped() {
        assert_eq!(grid_step(Some(99), GridDir::Right, COUNT, COLS), Some(6));
    }
}
