use crate::components::input::state::InputState;
use gpui::{
    App, AppContext, CursorStyle, Div, ElementId, Entity, Focusable, Hsla, InteractiveElement,
    Interactivity, IntoElement, MouseButton, ParentElement, Refineable, RenderOnce, SharedString,
    Stateful, StatefulInteractiveElement, StyleRefinement, Styled, Window, div,
    prelude::FluentBuilder, rems,
};

mod actions;
mod cursor;
mod element;
mod events;
mod history;
mod state;
#[cfg(test)]
mod tests;
mod text_ops;

pub(crate) use actions::init;
pub use events::*;
use theme_manager::ThemeExt;

/// Context identifier for text input key bindings
const CONTEXT: &str = "input";

pub fn input(id: impl Into<ElementId>) -> Input {
    let id = id.into();
    Input {
        id: id.clone(),
        base: div()
            .flex()
            .justify_center()
            .items_center()
            .id(id)
            .cursor(CursorStyle::IBeam),
        style: StyleRefinement::default(),
        disabled: false,
        value: None,
        on_input: None,
        on_change: None,
        placeholder: None,
        placeholder_color: None,
        selection_color: None,
        masked: false,
        mask: None,
        max_length: None,
        tab_index: 0,
        tab_stop: true,
    }
}

#[derive(IntoElement)]
pub struct Input {
    id: ElementId,
    base: Stateful<Div>,
    style: StyleRefinement,
    disabled: bool,
    value: Option<SharedString>,
    on_input: Option<Box<dyn Fn(&InputEvent, &mut Window, &mut App) + 'static>>,
    on_change: Option<Box<dyn Fn(&ChangeEvent, &mut Window, &mut App) + 'static>>,
    placeholder: Option<SharedString>,
    placeholder_color: Option<Hsla>,
    selection_color: Option<Hsla>,
    masked: bool,
    mask: Option<SharedString>,
    max_length: Option<usize>,
    tab_index: isize,
    tab_stop: bool,
}

impl Input {
    pub fn value(mut self, value: impl Into<SharedString>) -> Self {
        self.value = Some(value.into());
        self
    }

    pub fn on_input(
        mut self,
        callback: impl Fn(&InputEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_input = Some(Box::new(callback));
        self
    }

    pub fn on_change(
        mut self,
        callback: impl Fn(&ChangeEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Box::new(callback));
        self
    }

    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    pub fn masked(mut self, masked: bool) -> Self {
        self.masked = masked;
        self
    }

    pub fn mask(mut self, mask: impl Into<SharedString>) -> Self {
        self.mask = Some(mask.into());
        self
    }

    pub fn max_length(mut self, max_length: usize) -> Self {
        self.max_length = Some(max_length);
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
        let state = window
            .use_keyed_state(self.id, cx, |window, cx| {
                cx.new(|cx| InputState::new(window, cx))
            })
            .read(cx)
            .clone();

        let mut focus_handle = state.focus_handle(cx);
        if focus_handle.tab_stop != self.tab_stop {
            focus_handle = focus_handle.tab_stop(self.tab_stop);
        }
        if focus_handle.tab_index != self.tab_index {
            focus_handle = focus_handle.tab_index(self.tab_index);
        }

        state.update(cx, |state, _cx| {
            state.set_value(self.value);
            state.on_input = self.on_input;
            state.on_change = self.on_change;
            state.set_placeholder(self.placeholder);
            state.set_placeholder_color(self.placeholder_color);
            state.set_selection_color(self.selection_color);
            state.set_masked(self.masked);
            state.set_mask(self.mask);
            state.max_length = self.max_length;
        });

        let theme = cx.theme();

        let mut input = self
            .base
            .border_1()
            .rounded(rems(0.5))
            .bg(theme.colors.card_foreground)
            .p(rems(0.5))
            .when(!self.disabled, |this| {
                this.key_context(CONTEXT)
                    .track_focus(&focus_handle)
                    .on_action(window.listener_for(&state, InputState::backspace))
                    .on_action(window.listener_for(&state, InputState::delete))
                    .on_action(window.listener_for(&state, InputState::left))
                    .on_action(window.listener_for(&state, InputState::right))
                    .on_action(window.listener_for(&state, InputState::select_left))
                    .on_action(window.listener_for(&state, InputState::select_right))
                    .on_action(window.listener_for(&state, InputState::select_all))
                    .on_action(window.listener_for(&state, InputState::home))
                    .on_action(window.listener_for(&state, InputState::end))
                    .on_action(window.listener_for(&state, InputState::show_character_palette))
                    .on_action(window.listener_for(&state, InputState::paste))
                    .on_action(window.listener_for(&state, InputState::cut))
                    .on_action(window.listener_for(&state, InputState::copy))
                    .on_action(window.listener_for(&state, InputState::delete_word_left))
                    .on_action(window.listener_for(&state, InputState::delete_word_right))
                    .on_action(window.listener_for(&state, InputState::delete_to_beginning))
                    .on_action(window.listener_for(&state, InputState::delete_to_end))
                    .on_action(window.listener_for(&state, InputState::word_left))
                    .on_action(window.listener_for(&state, InputState::word_right))
                    .on_action(window.listener_for(&state, InputState::select_word_left))
                    .on_action(window.listener_for(&state, InputState::select_word_right))
                    .on_action(window.listener_for(&state, InputState::select_to_beginning))
                    .on_action(window.listener_for(&state, InputState::select_to_end))
                    .on_action(window.listener_for(&state, InputState::undo))
                    .on_action(window.listener_for(&state, InputState::redo))
                    .on_action(window.listener_for(&state, InputState::enter))
                    .on_mouse_down(
                        MouseButton::Left,
                        window.listener_for(&state, InputState::on_mouse_down),
                    )
                    .on_mouse_up(
                        MouseButton::Left,
                        window.listener_for(&state, InputState::on_mouse_up),
                    )
                    .on_mouse_up_out(
                        MouseButton::Left,
                        window.listener_for(&state, InputState::on_mouse_up),
                    )
                    .on_mouse_move(window.listener_for(&state, InputState::on_mouse_move))
            })
            .on_scroll_wheel(window.listener_for(&state, InputState::on_scroll_wheel))
            .focus(|input| {
                input
                    .border_1()
                    .border_color(theme.colors.foreground_primary)
            })
            .child(state.clone());

        input.style().refine(&self.style);
        input
    }
}
