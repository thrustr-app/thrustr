use gpui::{
    AnyElement, App, ClickEvent, ElementId, FontWeight, InteractiveElement, IntoElement,
    ParentElement, Refineable, RenderOnce, SharedString, StatefulInteractiveElement,
    StyleRefinement, Styled, Window, div, prelude::FluentBuilder, rems, transparent_black,
};
use smallvec::SmallVec;
use theme::ThemeExt;

#[allow(clippy::type_complexity)]
#[derive(IntoElement)]
pub struct Card {
    id: ElementId,
    style: StyleRefinement,
    header: Option<AnyElement>,
    children: SmallVec<[AnyElement; 1]>,
    on_click: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>>,
    auto_focus: bool,
    tab_index: isize,
    tab_stop: bool,
}

impl Card {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            style: StyleRefinement::default(),
            header: None,
            children: SmallVec::new(),
            on_click: None,
            auto_focus: false,
            tab_index: 0,
            tab_stop: true,
        }
    }

    pub fn title(self, title: impl Into<SharedString>) -> Self {
        self.header(
            div()
                .text_size(rems(1.25))
                .line_height(rems(1.25))
                .font_weight(FontWeight::MEDIUM)
                .child(title.into()),
        )
    }

    pub fn header(mut self, header: impl IntoElement) -> Self {
        self.header = Some(header.into_any_element());
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }

    pub fn auto_focus(mut self) -> Self {
        self.auto_focus = true;
        self
    }

    pub fn tab_stop(mut self, tab_stop: bool) -> Self {
        self.tab_stop = tab_stop;
        self
    }

    pub fn tab_index(mut self, tab_index: isize) -> Self {
        self.tab_index = tab_index;
        self
    }
}

impl Styled for Card {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for Card {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Card {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let mut focus_handle = window
            .use_keyed_state((self.id.clone(), "card"), cx, |window, cx| {
                let focus_handle = cx.focus_handle();
                if self.auto_focus {
                    focus_handle.focus(window, cx);
                }
                focus_handle
            })
            .read(cx)
            .clone();

        if focus_handle.tab_stop != self.tab_stop {
            focus_handle = focus_handle.tab_stop(self.tab_stop);
        }
        if focus_handle.tab_index != self.tab_index {
            focus_handle = focus_handle.tab_index(self.tab_index);
        }
        let theme = cx.theme();

        let mut card = div()
            .id((self.id.clone(), "card"))
            .bg(theme.colors.card_background)
            .overflow_hidden()
            .p(rems(1.5))
            .rounded(theme.radius.lg)
            .flex()
            .gap(rems(1.5))
            .flex_col()
            .text_color(theme.colors.card_primary)
            .border_1()
            .border_color(transparent_black())
            .when_some(self.header, |card, header| card.child(header))
            .when_some(self.on_click, |card, on_click| {
                card.cursor_pointer()
                    .on_click(on_click)
                    .track_focus(&focus_handle)
            })
            .focus(|card| card.border_color(theme.colors.primary))
            .children(self.children);

        card.style().refine(&self.style);
        card
    }
}
