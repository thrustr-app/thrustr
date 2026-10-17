// SPDX-License-Identifier: GPL-3.0-or-later
//
// Parts of this module are adapted from gpui-kit,
// Copyright (C) Longbridge, licensed under Apache-2.0:
// https://github.com/longbridge/gpui-kit/blob/main/crates/base/src/input/base/mod.rs
//
// Parts of this module are adapted from the gpui text-input example,
// Copyright (C) Zed Industries, Inc., licensed under Apache-2.0:
// https://github.com/zed-industries/zed/blob/main/crates/gpui/examples/input.rs
//
// Modified and redistributed as part of Thrustr under GPL-3.0-or-later.

use crate::{
    FieldProps, FocusProps, Icon, Radius, Size, WithField, WithFocus, WithRadius, WithSize,
};
use gpui::{
    App, AppContext, CursorStyle, Div, ElementId, Entity, FocusHandle, Focusable, FontWeight,
    InteractiveElement, Interactivity, IntoElement, MouseButton, ParentElement, Refineable,
    RenderOnce, SharedString, Stateful, StatefulInteractiveElement, StyleRefinement, Styled,
    Window, div, prelude::FluentBuilder, rems,
};

mod actions;
mod cursor;
mod element;
mod history;
mod state;
mod text_ops;

pub(crate) use actions::Backspace;
pub(super) use actions::init;
pub use state::{InputEvent, InputState};
use theme::ThemeExt;

const CONTEXT: &str = "input";

#[derive(IntoElement)]
pub struct Input {
    id: ElementId,
    state: Entity<InputState>,
    base: Stateful<Div>,
    style: StyleRefinement,
    disabled: bool,
    leading_icon: Option<Icon>,
    clear_button: bool,
    focus: FocusProps,
    field: FieldProps,
    radius: Radius,
    size: Size,
}

impl Input {
    pub fn new(state: &Entity<InputState>) -> Self {
        let id: ElementId = ("input", state.entity_id()).into();
        Self {
            id: id.clone(),
            state: state.clone(),
            base: div()
                .flex()
                .justify_center()
                .items_center()
                .id(id)
                .cursor(CursorStyle::IBeam),
            style: StyleRefinement::default(),
            disabled: false,
            leading_icon: None,
            clear_button: false,
            focus: FocusProps::default(),
            field: FieldProps::default(),
            radius: Radius::default(),
            size: Size::default(),
        }
    }

    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }

    pub fn leading_icon(mut self, icon: Icon) -> Self {
        self.leading_icon = Some(icon);
        self
    }

    pub fn clear_button(mut self) -> Self {
        self.clear_button = true;
        self
    }
}

impl WithFocus for Input {
    fn focus_props(&mut self) -> &mut FocusProps {
        &mut self.focus
    }
}

impl WithField for Input {
    fn field_props(&mut self) -> &mut FieldProps {
        &mut self.field
    }
}

impl WithRadius for Input {
    fn radius(mut self, radius: Radius) -> Self {
        self.radius = radius;
        self
    }
}

impl WithSize for Input {
    fn size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }
}

impl Styled for Input {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl InteractiveElement for Input {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.base.interactivity()
    }
}

impl StatefulInteractiveElement for Input {}

impl RenderOnce for Input {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = self.state;
        let focus_handle = self.focus.configure(state.focus_handle(cx), window, cx);

        let has_value = !state.read(cx).value.is_empty();

        let theme = cx.theme();

        let radius = match self.radius {
            Radius::Medium => theme.radius.md,
            Radius::Pill => theme.radius.pill,
        };
        let border_color = self.field.border_color(theme.colors.field.border, &theme);
        let hover_border_color = self.field.border_color(theme.colors.field.hover, &theme);
        let focus_border_color = self.field.border_color(theme.colors.field.focus, &theme);
        let focused = focus_handle.is_focused(window);

        let mut input = self
            .base
            .border_1()
            .border_color(border_color)
            .text_color(theme.colors.primary)
            .bg(theme.colors.field.background)
            .rounded(radius)
            .h(self.size.control_height())
            .px(self.size.field_px())
            .gap(rems(0.75))
            .text_size(theme.text.md)
            .font_weight(FontWeight::NORMAL)
            .when(!self.disabled && !focused, |input| {
                input.hover(|input| input.border_color(hover_border_color))
            })
            .when_some(self.leading_icon, |input, icon| {
                input.child(icon.size_sm().color(theme.colors.secondary))
            })
            .map(|input| editable(input, &state, &focus_handle, !self.disabled, window))
            .focus(|input| input.border_1().border_color(focus_border_color))
            .when(self.disabled, |button| button.opacity(0.6))
            .child(state.clone())
            .when(self.clear_button && has_value && !self.disabled, |input| {
                let state = state.clone();
                input.child(
                    div()
                        .id((self.id.clone(), "clear"))
                        .flex_none()
                        .cursor_pointer()
                        .child(Icon::x().size_sm().color(theme.colors.secondary))
                        .on_click(move |_, window, cx| {
                            state.update(cx, |state, cx| state.clear(window, cx));
                        }),
                )
            });

        input.style().refine(&self.style);

        let container = self.field.wrap(
            self.id.clone(),
            input,
            (!self.disabled).then(|| focus_handle.clone()),
        );

        self.focus
            .attach_reveal(container, &focus_handle, (self.id, "reveal"), window, cx)
    }
}

pub(crate) fn editable<E: InteractiveElement>(
    element: E,
    state: &Entity<InputState>,
    focus_handle: &FocusHandle,
    interactive: bool,
    window: &Window,
) -> E {
    let element = element.on_scroll_wheel(window.listener_for(state, InputState::on_scroll_wheel));
    if !interactive {
        return element;
    }
    element
        .key_context(CONTEXT)
        .track_focus(focus_handle)
        .on_action(window.listener_for(state, InputState::backspace))
        .on_action(window.listener_for(state, InputState::delete))
        .on_action(window.listener_for(state, InputState::left))
        .on_action(window.listener_for(state, InputState::right))
        .on_action(window.listener_for(state, InputState::select_left))
        .on_action(window.listener_for(state, InputState::select_right))
        .on_action(window.listener_for(state, InputState::select_all))
        .on_action(window.listener_for(state, InputState::home))
        .on_action(window.listener_for(state, InputState::end))
        .on_action(window.listener_for(state, InputState::show_character_palette))
        .on_action(window.listener_for(state, InputState::paste))
        .on_action(window.listener_for(state, InputState::cut))
        .on_action(window.listener_for(state, InputState::copy))
        .on_action(window.listener_for(state, InputState::delete_word_left))
        .on_action(window.listener_for(state, InputState::delete_word_right))
        .on_action(window.listener_for(state, InputState::delete_to_beginning))
        .on_action(window.listener_for(state, InputState::delete_to_end))
        .on_action(window.listener_for(state, InputState::word_left))
        .on_action(window.listener_for(state, InputState::word_right))
        .on_action(window.listener_for(state, InputState::select_word_left))
        .on_action(window.listener_for(state, InputState::select_word_right))
        .on_action(window.listener_for(state, InputState::select_to_beginning))
        .on_action(window.listener_for(state, InputState::select_to_end))
        .on_action(window.listener_for(state, InputState::undo))
        .on_action(window.listener_for(state, InputState::redo))
        .on_action(window.listener_for(state, InputState::enter))
        .on_mouse_down(
            MouseButton::Left,
            window.listener_for(state, InputState::on_mouse_down),
        )
        .on_mouse_up(
            MouseButton::Left,
            window.listener_for(state, InputState::on_mouse_up),
        )
        .on_mouse_up_out(
            MouseButton::Left,
            window.listener_for(state, InputState::on_mouse_up),
        )
        .on_mouse_move(window.listener_for(state, InputState::on_mouse_move))
}
