use gpui::{
    App, FontWeight, InteractiveElement, IntoElement, ParentElement, Pixels, Refineable,
    RenderOnce, StatefulInteractiveElement, StyleRefinement, Styled, Window, div, px, relative,
    rems,
};
use std::rc::Rc;
use theme::ThemeExt;

const ITEM_WIDTH: Pixels = px(16.);

pub const INDEX_RAIL_LETTERS: [&str; 27] = [
    "#", "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R",
    "S", "T", "U", "V", "W", "X", "Y", "Z",
];

pub fn index_rail_position(label: &str) -> Option<usize> {
    match label.as_bytes() {
        b"#" => Some(0),
        [c] if c.is_ascii_uppercase() => Some(1 + (c - b'A') as usize),
        _ => None,
    }
}

type SelectHandler = Rc<dyn Fn(&'static str, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct IndexRail {
    style: StyleRefinement,
    available: u32,
    current: Option<usize>,
    on_select: Option<SelectHandler>,
}

impl IndexRail {
    pub fn new() -> Self {
        Self {
            style: StyleRefinement::default(),
            available: 0,
            current: None,
            on_select: None,
        }
    }

    /// Bitmask over [`INDEX_RAIL_LETTERS`] marking which sections currently exist.
    pub fn available(mut self, mask: u32) -> Self {
        self.available = mask;
        self
    }

    pub fn current(mut self, index: Option<usize>) -> Self {
        self.current = index;
        self
    }

    pub fn on_select(
        mut self,
        handler: impl Fn(&'static str, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_select = Some(Rc::new(handler));
        self
    }
}

impl Default for IndexRail {
    fn default() -> Self {
        Self::new()
    }
}

impl Styled for IndexRail {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for IndexRail {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let unavailable = theme.colors.secondary.opacity(0.3);

        let mut rail = div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(rems(0.25))
            .children(INDEX_RAIL_LETTERS.iter().enumerate().map(|(i, &letter)| {
                let available = self.available & (1 << i) != 0;

                let color = if self.current == Some(i) {
                    theme.colors.accent
                } else if available {
                    theme.colors.tertiary
                } else {
                    unavailable
                };

                let cell = div()
                    .id(("index-rail-letter", i))
                    .w(ITEM_WIDTH)
                    .flex()
                    .justify_center()
                    .text_size(theme.text.sm)
                    .line_height(relative(1.5))
                    .font_weight(FontWeight::BOLD)
                    .text_color(color)
                    .child(letter);

                match (available, &self.on_select) {
                    (true, Some(on_select)) => {
                        let on_select = on_select.clone();
                        cell.cursor_pointer()
                            .on_click(move |_, window, cx| on_select(letter, window, cx))
                    }
                    _ => cell,
                }
            }));

        rail.style().refine(&self.style);
        rail
    }
}
