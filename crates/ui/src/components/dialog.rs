use crate::{Button, PortalContext, UiProvider, WithVariant};
use gpui::{
    AnyElement, App, ClickEvent, Div, FocusHandle, FontWeight, InteractiveElement, IntoElement,
    KeyBinding, MouseButton, ParentElement, RenderOnce, SharedString, Styled, Window, actions,
    anchored, div, prelude::FluentBuilder, px, rems,
};
use std::rc::Rc;
use theme_manager::ThemeExt;

const CONTEXT: &str = "dialog";

actions!(dialog, [CancelDialog, ConfirmDialog]);

pub(super) fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", CancelDialog, Some(CONTEXT)),
        KeyBinding::new("enter", ConfirmDialog, Some(CONTEXT)),
    ]);
}

#[allow(clippy::type_complexity)]
#[derive(IntoElement)]
pub struct Dialog {
    header: Option<AnyElement>,
    content: Div,
    on_cancel_handler: Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>,
    on_ok_handler: Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>,
    cancel_text: SharedString,
    ok_text: SharedString,
    overlay: bool,
    overlay_closable: bool,
    disabled: bool,
    pub(crate) focus_handle: FocusHandle,
    pub(crate) layer_ix: usize,
    pub(crate) overlay_visible: bool,
}

impl Dialog {
    pub fn new(cx: &mut App) -> Self {
        Self {
            header: None,
            content: div().flex().flex_col(),
            on_cancel_handler: Rc::new(|_, _, _| {}),
            on_ok_handler: Rc::new(|_, _, _| {}),
            cancel_text: "Cancel".into(),
            ok_text: "Ok".into(),
            overlay: true,
            overlay_closable: true,
            disabled: false,
            focus_handle: cx.focus_handle(),
            layer_ix: 0,
            overlay_visible: false,
        }
    }
    pub fn title(self, title: impl Into<SharedString>) -> Self {
        self.header(
            div()
                .text_size(px(24.))
                .line_height(px(24.))
                .font_weight(FontWeight::MEDIUM)
                .child(title.into()),
        )
    }

    pub fn header(mut self, header: impl IntoElement) -> Self {
        self.header = Some(header.into_any_element());
        self
    }

    pub fn overlay_closable(mut self, overlay_closable: bool) -> Self {
        self.overlay_closable = overlay_closable;
        self
    }

    pub fn on_cancel(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_cancel_handler = Rc::new(handler);
        self
    }

    pub fn on_ok(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_ok_handler = Rc::new(handler);
        self
    }

    pub fn cancel_text(mut self, text: impl Into<SharedString>) -> Self {
        self.cancel_text = text.into();
        self
    }

    pub fn ok_text(mut self, text: impl Into<SharedString>) -> Self {
        self.ok_text = text.into();
        self
    }

    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }

    pub(crate) fn has_overlay(&self) -> bool {
        self.overlay
    }
}

impl ParentElement for Dialog {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.content.extend(elements);
    }
}

impl RenderOnce for Dialog {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let on_cancel_handler = self.on_cancel_handler.clone();
        let on_ok_handler = self.on_ok_handler.clone();

        let viewport = window.viewport_size();

        if !self.focus_handle.contains_focused(window, cx) {
            self.focus_handle.focus(window, cx);
            window.focus_next(cx);
        }

        let theme = cx.theme();

        anchored().child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .h(viewport.height)
                .w(viewport.width)
                .when(self.overlay_visible, |this| {
                    this.occlude().bg(theme.colors.overlay)
                })
                .when(self.overlay_closable, |this| {
                    if (self.layer_ix + 1) != UiProvider::read(window, cx).active_dialogs.len() {
                        return this;
                    }

                    this.on_mouse_down(MouseButton::Left, {
                        let on_cancel_handler = on_cancel_handler.clone();
                        move |_, window, cx| {
                            on_cancel_handler(&ClickEvent::default(), window, cx);
                            window.close_dialog(cx);
                        }
                    })
                })
                .child(
                    div()
                        .id(("dialog", self.layer_ix))
                        .key_context(CONTEXT)
                        .track_focus(&self.focus_handle)
                        .tab_group()
                        .tab_index(self.layer_ix as isize)
                        .tab_stop(false)
                        .rounded(theme.radius.lg)
                        .bg(theme.colors.background)
                        .p(rems(1.5))
                        .absolute()
                        .occlude()
                        .relative()
                        .flex()
                        .gap(rems(1.5))
                        .flex_col()
                        .text_color(theme.colors.card_primary)
                        .w_auto()
                        .h_auto()
                        .on_action({
                            let on_close_handler = on_cancel_handler.clone();
                            move |_: &CancelDialog, window, cx| {
                                on_close_handler(&ClickEvent::default(), window, cx);
                                window.close_dialog(cx);
                            }
                        })
                        .children(self.header)
                        .child(self.content)
                        .child(
                            div()
                                .flex()
                                .gap(rems(1.))
                                .flex_row_reverse()
                                .justify_end()
                                .child(
                                    Button::new("ok-dialog")
                                        .w_full()
                                        .max_w(rems(10.))
                                        .variant_accent()
                                        .child(self.ok_text)
                                        .when(self.disabled, Button::disabled)
                                        .on_click({
                                            move |event, window, cx| {
                                                on_ok_handler(event, window, cx);
                                                window.close_dialog(cx);
                                            }
                                        }),
                                )
                                .child(
                                    Button::new("close-dialog")
                                        .w_full()
                                        .max_w(rems(10.))
                                        .variant_ghost()
                                        .child(self.cancel_text)
                                        .on_click({
                                            move |event, window, cx| {
                                                on_cancel_handler(event, window, cx);
                                                window.close_dialog(cx);
                                            }
                                        }),
                                ),
                        ),
                ),
        )
    }
}
