use crate::{
    FieldProps, FocusProps, Icon, ListScrollbar, SCROLLBAR_WIDTH, Size, WithField, WithFocus,
    WithSize,
    components::{
        input::editable,
        menu::{
            self, BORDER, Cancel, Confirm, MENU_MAX_HEIGHT, MENU_PADDING, SelectNext,
            SelectNextPage, SelectPrev, SelectPrevPage,
        },
    },
};
use gpui::{
    App, ClickEvent, CursorStyle, Div, ElementId, Entity, Focusable, FontWeight,
    InteractiveElement, IntoElement, KeyBinding, ListSizingBehavior, MouseButton, MouseDownEvent,
    MouseMoveEvent, ParentElement, Refineable, Rems, RenderOnce, SharedString,
    StatefulInteractiveElement, StyleRefinement, Styled, Window, div, prelude::FluentBuilder, px,
    relative, rems, svg, uniform_list,
};
pub use state::{ComboboxEvent, ComboboxState, SelectionMode};
use theme::{Theme, ThemeExt};

mod state;

const CONTEXT: &str = "combobox";

const MIN_SEARCH_WIDTH: Rems = rems(4.);
const LIST_MAX_HEIGHT: Rems = rems(MENU_MAX_HEIGHT.0 - MENU_PADDING.0 * 2.);

const REMOVE_GROUP: &str = "combobox-chip-remove";
const REMOVE_SIZE: Rems = rems(0.875);
const REMOVE_ICON_SIZE: Rems = rems(0.75);

pub(super) fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("up", SelectPrev, Some(CONTEXT)),
        KeyBinding::new("down", SelectNext, Some(CONTEXT)),
        KeyBinding::new("pageup", SelectPrevPage, Some(CONTEXT)),
        KeyBinding::new("pagedown", SelectNextPage, Some(CONTEXT)),
        KeyBinding::new("enter", Confirm, Some(CONTEXT)),
        KeyBinding::new("escape", Cancel, Some(CONTEXT)),
    ]);
}

#[derive(IntoElement)]
pub struct Combobox<T: Clone + PartialEq + 'static> {
    id: ElementId,
    state: Entity<ComboboxState<T>>,
    style: StyleRefinement,
    size: Size,
    empty_placeholder: Option<SharedString>,
    focus: FocusProps,
    field: FieldProps,
    disabled: bool,
}

impl<T: Clone + PartialEq + 'static> Combobox<T> {
    pub fn new(state: &Entity<ComboboxState<T>>) -> Self {
        Self {
            id: ("combobox", state.entity_id()).into(),
            state: state.clone(),
            style: StyleRefinement::default(),
            size: Size::default(),
            empty_placeholder: None,
            focus: FocusProps::default(),
            field: FieldProps::default(),
            disabled: false,
        }
    }

    pub fn empty_placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.empty_placeholder = Some(placeholder.into());
        self
    }

    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }
}

impl<T: Clone + PartialEq + 'static> Styled for Combobox<T> {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl<T: Clone + PartialEq + 'static> WithSize for Combobox<T> {
    fn size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }
}

impl<T: Clone + PartialEq + 'static> WithFocus for Combobox<T> {
    fn focus_props(&mut self) -> &mut FocusProps {
        &mut self.focus
    }
}

impl<T: Clone + PartialEq + 'static> WithField for Combobox<T> {
    fn field_props(&mut self) -> &mut FieldProps {
        &mut self.field
    }
}

impl<T: Clone + PartialEq + 'static> RenderOnce for Combobox<T> {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = self.state;
        let theme = cx.theme();
        let interactive = !self.disabled;

        state.update(cx, |state, cx| state.prepare(interactive, cx));

        let (input, open, multiple, focus_handle) = {
            let state = state.read(cx);
            (
                state.input.clone(),
                state.is_open && interactive,
                state.is_multiple(),
                state.focus_handle(cx),
            )
        };
        let focus_handle = self.focus.configure(focus_handle, window, cx);

        let border_color = self.field.border_color(theme.colors.field.border, &theme);
        let hover_border_color = self.field.border_color(theme.colors.field.hover, &theme);
        let focus_border_color = self.field.border_color(theme.colors.field.focus, &theme);
        let focused = focus_handle.is_focused(window) || open;

        let search = div()
            .flex()
            .items_center()
            .flex_1()
            .cursor(CursorStyle::IBeam);
        let search = editable(search, &input, &focus_handle, interactive, window).child(input);

        let indicator = div()
            .flex_none()
            .child(Icon::chevron().size_md().color(theme.colors.secondary))
            .when(interactive, |indicator| {
                indicator.cursor_pointer().on_mouse_down(
                    MouseButton::Left,
                    window.listener_for(&state, |state, _: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        state.toggle(window, cx);
                    }),
                )
            });

        let mut field = div()
            .id(self.id.clone())
            .relative()
            .flex()
            .items_center()
            .gap(rems(0.5))
            .rounded(theme.radius.md)
            .border_1()
            .border_color(border_color)
            .bg(theme.colors.field.background)
            .text_color(theme.colors.primary)
            .text_size(theme.text.md)
            .font_weight(FontWeight::NORMAL)
            .when(focused, |field| field.border_color(focus_border_color))
            .when(self.disabled, |field| field.opacity(0.6))
            .map(|field| match multiple {
                true => {
                    let chips = render_chips(&state, self.size, interactive, &theme, window, cx);
                    field.pr(self.size.field_px()).child(
                        chips.child(search.h(chip_height(self.size)).min_w(MIN_SEARCH_WIDTH)),
                    )
                }
                false => field
                    .h(self.size.control_height())
                    .px(self.size.field_px())
                    .child(search.min_w_0()),
            })
            .child(indicator)
            .when(interactive, |field| {
                field
                    .when(!focused, |field| {
                        field.hover(|field| field.border_color(hover_border_color))
                    })
                    .key_context(CONTEXT)
                    .on_mouse_down(
                        MouseButton::Left,
                        window.listener_for(&state, |state, _: &MouseDownEvent, window, cx| {
                            state.focus_handle(cx).focus(window, cx);
                            state.open(cx);
                        }),
                    )
                    .on_action(window.listener_for(&state, ComboboxState::select_prev))
                    .on_action(window.listener_for(&state, ComboboxState::select_next))
                    .on_action(window.listener_for(&state, ComboboxState::select_prev_page))
                    .on_action(window.listener_for(&state, ComboboxState::select_next_page))
                    .on_action(window.listener_for(&state, ComboboxState::confirm))
                    .when(multiple, |field| {
                        field.capture_action(window.listener_for(&state, ComboboxState::backspace))
                    })
                    .when(open, |field| {
                        field
                            .on_action(window.listener_for(&state, ComboboxState::cancel))
                            .children(render_menu(
                                &state,
                                self.size,
                                self.empty_placeholder,
                                window,
                                cx,
                            ))
                    })
                    .child({
                        let state = state.clone();
                        menu::track_bounds(move |bounds, cx| {
                            state.update(cx, |state, cx| state.set_bounds(bounds, cx));
                        })
                    })
            });

        field.style().refine(&self.style);

        let container = self.field.wrap(
            self.id.clone(),
            field,
            interactive.then(|| focus_handle.clone()),
        );

        self.focus
            .attach_reveal(container, &focus_handle, (self.id, "reveal"), window, cx)
    }
}

fn chip_height(size: Size) -> Rems {
    match size {
        Size::Small => rems(1.25),
        Size::Medium => rems(1.5),
        Size::Large => rems(1.625),
        Size::ExtraLarge => rems(1.75),
    }
}

fn render_chips<T: Clone + PartialEq + 'static>(
    state: &Entity<ComboboxState<T>>,
    size: Size,
    interactive: bool,
    theme: &Theme,
    window: &Window,
    cx: &App,
) -> Div {
    let rem_size = window.rem_size();
    let chip_height = chip_height(size);
    let inset =
        (size.control_height().to_pixels(rem_size) - chip_height.to_pixels(rem_size)) / 2. - BORDER;
    let chip_radius = (theme.radius.md.to_pixels(rem_size) - BORDER - inset).max(px(4.));

    let current = state.read(cx);
    let chips = current.selected.iter().enumerate().map(|(pos, &item_ix)| {
        let remove = div()
            .id(("remove", pos))
            .group(REMOVE_GROUP)
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .size(REMOVE_SIZE)
            .child(
                svg()
                    .path(Icon::x().path())
                    .size(REMOVE_ICON_SIZE)
                    .text_color(theme.colors.secondary)
                    .when(interactive, |icon| {
                        icon.group_hover(REMOVE_GROUP, |icon| icon.text_color(theme.colors.primary))
                    }),
            )
            .when(interactive, |remove| {
                remove
                    .cursor_pointer()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(
                        window.listener_for(state, move |state, _: &ClickEvent, _, cx| {
                            state.remove(pos, cx)
                        }),
                    )
            });

        div()
            .flex()
            .items_center()
            .gap(rems(0.125))
            .max_w_full()
            .h(chip_height)
            .pl(rems(0.375))
            .pr(rems(0.25))
            .rounded(chip_radius)
            .bg(theme.colors.secondary_background)
            .text_size(theme.text.sm)
            .line_height(relative(1.))
            .font_weight(FontWeight::MEDIUM)
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .child(current.items[item_ix].1.clone()),
            )
            .child(remove)
    });

    let padding_left = match current.selected.is_empty() {
        true => size.field_px().to_pixels(rem_size),
        false => inset,
    };

    div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(rems(0.25))
        .py(inset)
        .pl(padding_left)
        .children(chips)
}

fn render_menu<T: Clone + PartialEq + 'static>(
    state: &Entity<ComboboxState<T>>,
    size: Size,
    empty_placeholder: Option<SharedString>,
    window: &Window,
    cx: &App,
) -> Option<impl IntoElement> {
    let theme = cx.theme();
    let current = state.read(cx);
    let scrollable = current.list_overflows(LIST_MAX_HEIGHT.to_pixels(window.rem_size()));

    let content = if current.matches.is_empty() {
        menu::item(empty_placeholder?, size, false, false, &theme)
            .cursor_default()
            .text_color(theme.colors.field.placeholder)
            .into_any_element()
    } else {
        let list_state = state.clone();
        uniform_list("items", current.matches.len(), move |range, window, cx| {
            let theme = cx.theme();
            let current = list_state.read(cx);
            range
                .map(|ix| {
                    let item_ix = current.matches[ix];
                    menu::item(
                        div().truncate().child(current.items[item_ix].1.clone()),
                        size,
                        current.highlighted == Some(ix),
                        current.is_selected(item_ix),
                        &theme,
                    )
                    .w_full()
                    .id(item_ix)
                    .on_mouse_move(
                        window
                            .listener_for(&list_state, move |state, _: &MouseMoveEvent, _, cx| {
                                state.highlight(ix, cx)
                            }),
                    )
                    .on_click(
                        window.listener_for(&list_state, move |state, _: &ClickEvent, _, cx| {
                            state.choose(ix, cx)
                        }),
                    )
                })
                .collect()
        })
        .with_sizing_behavior(ListSizingBehavior::Infer)
        .max_h(LIST_MAX_HEIGHT)
        // Keep the rows clear of the scrollbar, which sits in the padding.
        .when(scrollable, |list| list.pr(SCROLLBAR_WIDTH))
        .track_scroll(&current.list)
        .with_decoration(ListScrollbar::new(current.scrollbar.clone()))
        .into_any_element()
    };

    Some(menu::popover(
        current.bounds.size.width,
        div()
            .p(MENU_PADDING)
            .when(scrollable, |content| content.pr_0())
            .child(content),
        window.listener_for(state, ComboboxState::dismiss),
        window,
        cx,
    ))
}
