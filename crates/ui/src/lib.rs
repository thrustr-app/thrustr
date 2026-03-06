use gpui::{
    AnyView, App, AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement,
    IntoElement, KeyBinding, ParentElement, Render, Styled, Window, actions, div,
};

mod components;
mod traits;

pub use components::*;
pub use traits::*;

actions!(global, [Tab, TabPrev]);

pub struct UiProvider {
    view: AnyView,
    focus_handle: FocusHandle,
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
        })
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
