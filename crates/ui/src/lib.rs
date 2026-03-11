use std::rc::Rc;

use gpui::{
    AnyView, App, AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement,
    IntoElement, KeyBinding, ParentElement, Render, Styled, Window, actions, div,
};

mod components;
mod traits;

pub use components::*;
pub use traits::*;

actions!(global, [Tab, TabPrev]);

#[allow(clippy::type_complexity)]
#[derive(Clone)]
pub(crate) struct ActiveDialog {
    focus_handle: FocusHandle,
    builder: Rc<dyn Fn(Dialog, &mut Window, &mut App) -> Dialog + 'static>,
}

pub struct UiProvider {
    view: AnyView,
    focus_handle: FocusHandle,
    previous_focus_handle: Option<FocusHandle>,
    pub(crate) active_dialogs: Vec<ActiveDialog>,
}

impl UiProvider {
    pub fn new(view: impl Into<AnyView>, _window: &mut Window, cx: &mut App) -> Entity<Self> {
        components::init(cx);
        cx.bind_keys([
            KeyBinding::new("tab", Tab, None),
            KeyBinding::new("shift-tab", TabPrev, None),
        ]);

        let view = view.into();
        cx.new(|cx| UiProvider {
            view,
            focus_handle: cx.focus_handle(),
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

    fn focus_back(&mut self, window: &mut Window, _: &mut App) {
        if let Some(handle) = self.previous_focus_handle.clone() {
            window.focus(&handle);
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
                let mut dialog = Dialog::new(window, cx);

                dialog = (active_dialog.builder)(dialog, window, cx);
                dialog.focus_handle = active_dialog.focus_handle.clone();

                dialog.layer_ix = i;
                if dialog.has_overlay() {
                    show_overlay_ix = Some(i);
                }

                dialog
            })
            .collect::<Vec<_>>();

        if let Some(ix) = show_overlay_ix {
            if let Some(dialog) = dialogs.get_mut(ix) {
                dialog.overlay_visible = true;
            }
        }

        Some(div().children(dialogs))
    }

    fn on_tab(&mut self, _: &Tab, window: &mut Window, _: &mut Context<Self>) {
        window.focus_next();
    }

    fn on_tab_prev(&mut self, _: &TabPrev, window: &mut Window, _: &mut Context<Self>) {
        window.focus_prev();
    }
}

impl Focusable for UiProvider {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for UiProvider {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .track_focus(&self.focus_handle(cx))
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
            focus_handle.focus(window);

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
                top_dialog.focus_handle.focus(window);
            } else {
                root.focus_back(window, cx);
            }
            cx.notify();
        })
    }
}
