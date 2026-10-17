use crate::{Icon, Size, WithSize};
use gpui::{
    Animation, AnimationExt, App, Bounds, Div, InteractiveElement, IntoElement, MouseDownEvent,
    ParentElement, Pixels, Rems, Styled, Window, actions, anchored, canvas, deferred, div,
    ease_out_quint, point, prelude::FluentBuilder, px, relative, rems,
};
use std::time::Duration;
use theme::{Theme, ThemeExt};

pub(crate) const BORDER: Pixels = px(1.);
pub(crate) const MENU_PADDING: Rems = rems(0.25);
pub(crate) const MENU_MAX_HEIGHT: Rems = rems(16.);
const MENU_GAP: Rems = rems(0.25);
const MENU_SLIDE: Pixels = px(6.);
const MENU_OPEN_DURATION: Duration = Duration::from_millis(150);

actions!(
    menu,
    [
        SelectPrev,
        SelectNext,
        SelectFirst,
        SelectLast,
        SelectPrevPage,
        SelectNextPage,
        Confirm,
        Cancel
    ]
);

fn item_px(size: Size) -> Rems {
    rems(size.field_px().0 - MENU_PADDING.0)
}

fn item_py(size: Size) -> Rems {
    match size {
        Size::Small => rems(0.25),
        Size::Medium => rems(0.375),
        Size::Large => rems(0.5),
        Size::ExtraLarge => rems(0.625),
    }
}

pub(crate) fn item(
    content: impl IntoElement,
    size: Size,
    highlighted: bool,
    selected: bool,
    theme: &Theme,
) -> Div {
    div()
        .flex()
        .items_center()
        .gap(rems(0.5))
        .px(item_px(size))
        .py(item_py(size))
        .rounded(theme.radius.sm)
        .cursor_pointer()
        .when(highlighted, |item| item.bg(theme.colors.popover.highlight))
        .child(div().flex_1().min_w_0().child(content))
        .child(
            div()
                .flex_none()
                .size(Icon::length(Size::Small))
                .when(selected, |slot| {
                    slot.child(Icon::check().size_sm().color(theme.colors.accent))
                }),
        )
}

pub(crate) fn track_bounds(
    on_bounds: impl Fn(Bounds<Pixels>, &mut App) + 'static,
) -> impl IntoElement {
    canvas(
        // The canvas covers the padding box, so grow it over the border.
        move |bounds, _, cx| on_bounds(bounds.dilate(BORDER), cx),
        |_, _, _, _| {},
    )
    .absolute()
    .inset_0()
}

pub(crate) fn popover(
    width: Pixels,
    content: impl IntoElement,
    on_dismiss: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    window: &Window,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let gap = MENU_GAP.to_pixels(window.rem_size());

    let menu = div()
        .relative()
        .w(width)
        .occlude()
        .rounded(theme.radius.md)
        .border_1()
        .border_color(theme.colors.popover.border)
        .bg(theme.colors.popover.background)
        .shadow_md()
        .line_height(relative(1.1))
        .on_mouse_down_out(on_dismiss)
        .child(content)
        .with_animation(
            "menu",
            Animation::new(MENU_OPEN_DURATION).with_easing(ease_out_quint()),
            |menu, delta| menu.opacity(delta).top(MENU_SLIDE * (delta - 1.)),
        );

    div().absolute().top_full().left(-BORDER).child(deferred(
        anchored()
            .offset(point(px(0.), BORDER + gap))
            .snap_to_window_with_margin(gap)
            .child(menu),
    ))
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Step {
    Prev,
    Next,
    First,
    Last,
    PrevPage,
    NextPage,
}

impl Step {
    pub(crate) fn target(self, current: Option<usize>, count: usize, page: usize) -> usize {
        let last = count - 1;
        let current = current.map(|ix| ix.min(last));

        match self {
            Self::First => 0,
            Self::Last => last,
            Self::Prev => current.map_or(last, |ix| ix.saturating_sub(1)),
            Self::Next => current.map_or(0, |ix| (ix + 1).min(last)),
            Self::PrevPage => current.map_or(last, |ix| ix.saturating_sub(page)),
            Self::NextPage => current.map_or(0, |ix| ix.saturating_add(page).min(last)),
        }
    }
}
