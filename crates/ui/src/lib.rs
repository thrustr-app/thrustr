use gpui::{
    AnyView, App, AppContext, Context, Entity, FocusHandle, InteractiveElement, IntoElement,
    KeyBinding, ParentElement, Render, Styled, Window, actions, div,
};
use std::rc::Rc;

mod components;
mod foundation;
mod interaction;

pub use components::*;
pub use foundation::*;
pub use interaction::grid::*;

actions!(global, [Tab, TabPrev]);

#[allow(clippy::type_complexity)]
#[derive(Clone)]
pub(crate) struct ActiveDialog {
    focus_handle: FocusHandle,
    builder: Rc<dyn Fn(Dialog, &mut Window, &mut App) -> Dialog + 'static>,
}

pub struct UiProvider {
    view: AnyView,
    previous_focus_handle: Option<FocusHandle>,
    pub(crate) active_dialogs: Vec<ActiveDialog>,
}

impl UiProvider {
    pub fn new(view: impl Into<AnyView>, _window: &mut Window, cx: &mut App) -> Entity<Self> {
        components::init(cx);
        interaction::init(cx);
        cx.bind_keys([
            KeyBinding::new("tab", Tab, None),
            KeyBinding::new("shift-tab", TabPrev, None),
        ]);

        let view = view.into();
        cx.new(|_| UiProvider {
            view,
            previous_focus_handle: None,
            active_dialogs: Vec::new(),
        })
    }

    pub fn update<F>(window: &mut Window, cx: &mut App, f: F)
    where
        F: FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    {
        if let Some(Some(provider)) = window.root::<Self>() {
            provider.update(cx, |provider, cx| f(provider, window, cx));
        }
    }

    pub fn read<'a>(window: &'a Window, cx: &'a App) -> &'a Self {
        window
            .root::<Self>()
            .expect("The window root view should be of type `ui::UiProvider`.")
            .unwrap()
            .read(cx)
    }

    fn focus_back(&mut self, window: &mut Window, cx: &mut App) {
        if let Some(handle) = self.previous_focus_handle.clone() {
            window.focus(&handle, cx);
        }
    }

    pub fn render_dialogs(window: &mut Window, cx: &mut App) -> Option<impl IntoElement> {
        let root = window.root::<Self>()??;

        let active_dialogs = root.read(cx).active_dialogs.clone();

        if active_dialogs.is_empty() {
            return None;
        }

        let mut show_overlay_ix = None;

        let mut dialogs = active_dialogs
            .iter()
            .enumerate()
            .map(|(i, active_dialog)| {
                let mut dialog = Dialog::new(cx);

                dialog = (active_dialog.builder)(dialog, window, cx);
                dialog.focus_handle = active_dialog.focus_handle.clone();

                dialog.layer_ix = i;
                if dialog.has_overlay() {
                    show_overlay_ix = Some(i);
                }

                dialog
            })
            .collect::<Vec<_>>();

        if let Some(ix) = show_overlay_ix
            && let Some(dialog) = dialogs.get_mut(ix)
        {
            dialog.overlay_visible = true;
        }

        Some(div().children(dialogs))
    }

    fn on_tab(&mut self, _: &Tab, window: &mut Window, cx: &mut Context<Self>) {
        self.cycle_focus(false, window, cx);
    }

    fn on_tab_prev(&mut self, _: &TabPrev, window: &mut Window, cx: &mut Context<Self>) {
        self.cycle_focus(true, window, cx);
    }

    /// Move focus to the next/previous tab stop, trapping focus inside the
    /// topmost open dialog so Tab/Shift-Tab cycle within it instead of escaping.
    fn cycle_focus(&mut self, backward: bool, window: &mut Window, cx: &mut App) {
        let Some(trap) = self.active_dialogs.last().map(|d| d.focus_handle.clone()) else {
            if backward {
                window.focus_prev(cx);
            } else {
                window.focus_next(cx);
            }
            return;
        };

        if backward {
            window.focus_prev(cx);
        } else {
            window.focus_next(cx);
        }

        if trap.contains_focused(window, cx) {
            return;
        }

        if backward {
            focus_last_child(&trap, window, cx);
        } else {
            trap.focus(window, cx);
            window.focus_next(cx);
        }
    }
}

fn focus_last_child(trap: &FocusHandle, window: &mut Window, cx: &mut App) {
    trap.focus(window, cx);

    let mut first: Option<FocusHandle> = None;
    let mut last: Option<FocusHandle> = None;

    loop {
        window.focus_next(cx);

        if !trap.contains_focused(window, cx) {
            break;
        }

        let focused = window.focused(cx);
        if focused == last || focused == first {
            break;
        }
        if first.is_none() {
            first = focused.clone();
        }

        last = focused;
    }

    if let Some(last) = last {
        window.focus(&last, cx);
    }
}

impl Render for UiProvider {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(self.view.clone())
            .id("ui-provider")
            .on_action(cx.listener(Self::on_tab))
            .on_action(cx.listener(Self::on_tab_prev))
    }
}

pub trait PortalContext: Sized {
    fn open_dialog<F>(&mut self, cx: &mut App, build: F)
    where
        F: Fn(Dialog, &mut Window, &mut App) -> Dialog + 'static;

    fn close_dialog(&mut self, cx: &mut App);
}

impl PortalContext for Window {
    fn open_dialog<F>(&mut self, cx: &mut App, build: F)
    where
        F: Fn(Dialog, &mut Window, &mut App) -> Dialog + 'static,
    {
        UiProvider::update(self, cx, move |root, window, cx| {
            if root.active_dialogs.is_empty() {
                root.previous_focus_handle = window.focused(cx);
            }

            let focus_handle = cx.focus_handle();
            focus_handle.focus(window, cx);

            root.active_dialogs.push(ActiveDialog {
                focus_handle,
                builder: Rc::new(build),
            });
            cx.notify();
        })
    }

    fn close_dialog(&mut self, cx: &mut App) {
        UiProvider::update(self, cx, move |root, window, cx| {
            root.active_dialogs.pop();

            if let Some(top_dialog) = root.active_dialogs.last() {
                top_dialog.focus_handle.focus(window, cx);
            } else {
                root.focus_back(window, cx);
            }
            cx.notify();
        })
    }
}
