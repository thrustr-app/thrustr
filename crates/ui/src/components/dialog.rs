use crate::{Alert, Button, PortalContext, UiProvider, WithSize, WithVariant};
use gpui::{
    Animation, AnimationExt, AnyElement, App, ClickEvent, Div, FocusHandle, FontWeight,
    InteractiveElement, IntoElement, KeyBinding, MouseButton, ParentElement, Refineable,
    RenderOnce, SharedString, StyleRefinement, Styled, Window, actions, anchored, div,
    ease_out_quint, prelude::FluentBuilder, px, relative, rems,
};
use std::rc::Rc;
use std::time::Duration;
use theme::ThemeExt;

const CONTEXT: &str = "dialog";

actions!(dialog, [CancelDialog, ConfirmDialog]);

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

pub(super) fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", CancelDialog, Some(CONTEXT)),
        KeyBinding::new("enter", ConfirmDialog, Some(CONTEXT)),
    ]);
}

enum Header {
    Title(SharedString),
    Custom(AnyElement),
}

#[derive(IntoElement)]
pub struct Dialog {
    header: Option<Header>,
    content: Div,
    on_cancel_handler: ClickHandler,
    on_ok_handler: ClickHandler,
    cancel_text: SharedString,
    ok_text: SharedString,
    error: Option<SharedString>,
    overlay: bool,
    overlay_closable: bool,
    disabled: bool,
    loading: bool,
    style: StyleRefinement,
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
            error: None,
            overlay: true,
            overlay_closable: true,
            disabled: false,
            loading: false,
            style: StyleRefinement::default(),
            focus_handle: cx.focus_handle(),
            layer_ix: 0,
            overlay_visible: false,
        }
    }

    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.header = Some(Header::Title(title.into()));
        self
    }

    pub fn header(mut self, header: impl IntoElement) -> Self {
        self.header = Some(Header::Custom(header.into_any_element()));
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

    pub fn error(mut self, error: impl Into<SharedString>) -> Self {
        self.error = Some(error.into());
        self
    }

    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }

    pub fn loading(mut self) -> Self {
        self.loading = true;
        self
    }

    pub(crate) fn has_overlay(&self) -> bool {
        self.overlay
    }
}

impl Styled for Dialog {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for Dialog {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.content.extend(elements);
    }
}

impl RenderOnce for Dialog {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let viewport = window.viewport_size();

        if self.loading {
            self.focus_handle.focus(window, cx);
        } else if !self.focus_handle.contains_focused(window, cx) {
            self.focus_handle.focus(window, cx);
            window.focus_next(cx);
        }

        let theme = cx.theme();

        let header = self.header.map(|header| match header {
            Header::Title(title) => div()
                .w_full()
                .text_center()
                .text_size(theme.text.lg)
                .line_height(relative(1.))
                .font_weight(FontWeight::BOLD)
                .child(title)
                .into_any_element(),
            Header::Custom(element) => element,
        });

        let cancel_handler = self.on_cancel_handler.clone();
        let loading = self.loading;
        let cancel = move |event: &ClickEvent, window: &mut Window, cx: &mut App| {
            if loading {
                return;
            }
            cancel_handler(event, window, cx);
            window.close_dialog(cx);
        };

        let ok_handler = self.on_ok_handler.clone();
        let disabled = self.disabled;
        let confirm = move |event: &ClickEvent, window: &mut Window, cx: &mut App| {
            if disabled || loading {
                return;
            }
            ok_handler(event, window, cx);
        };

        let mut dialog = div()
            .id(("dialog", self.layer_ix))
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .tab_group()
            .tab_index(self.layer_ix as isize)
            .tab_stop(false)
            .rounded(theme.radius.lg)
            .bg(theme.colors.background)
            .border_1()
            .border_color(theme.colors.border)
            .p(rems(2.))
            .occlude()
            .flex()
            .gap(rems(2.))
            .flex_col()
            .text_color(theme.colors.primary)
            .w_auto()
            .h_auto()
            .relative()
            .on_action({
                let cancel = cancel.clone();
                move |_: &CancelDialog, window, cx| cancel(&ClickEvent::default(), window, cx)
            })
            .on_action({
                let confirm = confirm.clone();
                move |_: &ConfirmDialog, window, cx| confirm(&ClickEvent::default(), window, cx)
            })
            .children(header)
            .children(self.error.map(Alert::new))
            .child(self.content)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(rems(1.))
                    .child(
                        Button::new("ok-dialog")
                            .size_lg()
                            .w_full()
                            .variant_accent()
                            .child(self.ok_text)
                            .when(self.disabled, Button::disabled)
                            .when(self.loading, Button::loading)
                            .on_click(confirm),
                    )
                    .child(
                        Button::new("close-dialog")
                            .size_lg()
                            .variant_ghost()
                            .child(self.cancel_text)
                            .when(self.loading, Button::disabled)
                            .on_click(cancel.clone()),
                    ),
            );

        dialog.style().refine(&self.style);

        let dialog = dialog.with_animation(
            ("dialog-open", self.layer_ix),
            Animation::new(Duration::from_millis(400)).with_easing(ease_out_quint()),
            |dialog, delta| dialog.opacity(delta).top(px((1.0 - delta) * 6.0)),
        );

        let container = div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .h(viewport.height)
            .w(viewport.width)
            .when(self.overlay_visible, |this| this.occlude())
            .when(self.overlay_closable, |this| {
                if (self.layer_ix + 1) != UiProvider::read(window, cx).active_dialogs.len() {
                    return this;
                }

                this.on_mouse_down(MouseButton::Left, {
                    let cancel = cancel;
                    move |_, window, cx| cancel(&ClickEvent::default(), window, cx)
                })
            })
            .child(dialog);

        let container = if self.overlay_visible {
            let overlay_color = theme.colors.overlay;
            container
                .with_animation(
                    ("dialog-overlay", self.layer_ix),
                    Animation::new(Duration::from_millis(150)),
                    move |this, delta| this.bg(overlay_color.opacity(delta)),
                )
                .into_any_element()
        } else {
            container.into_any_element()
        };

        anchored().child(container)
    }
}
