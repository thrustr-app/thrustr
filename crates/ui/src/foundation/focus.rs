use gpui::{
    App, Bounds, Context, ElementId, FocusHandle, ParentElement, Pixels, ScrollHandle, Styled,
    Subscription, Window, canvas, point, px,
};
use std::ops::Range;

/// Keyboard-focus behavior shared by focusable components.
pub struct FocusProps {
    pub(crate) auto_focus: bool,
    tab_index: isize,
    tab_stop: bool,
    reveal_in: Option<ScrollHandle>,
}

impl Default for FocusProps {
    fn default() -> Self {
        Self {
            auto_focus: false,
            tab_index: 0,
            tab_stop: true,
            reveal_in: None,
        }
    }
}

impl FocusProps {
    /// Apply the configured tab order to `handle`.
    pub(crate) fn configure(&self, handle: FocusHandle) -> FocusHandle {
        handle.tab_stop(self.tab_stop).tab_index(self.tab_index)
    }

    /// Wire reveal-on-focus onto a rendered component.
    pub(crate) fn attach_reveal<E>(
        &self,
        element: E,
        focus_handle: &FocusHandle,
        key: impl Into<ElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> E
    where
        E: ParentElement,
    {
        let Some(scroll_handle) = self.reveal_in.clone() else {
            return element;
        };

        let reveal = window.use_keyed_state(key.into(), cx, |window, cx| {
            Reveal::new(focus_handle, scroll_handle.clone(), window, cx)
        });

        element.child(
            canvas(
                move |bounds, _, cx| {
                    reveal.update(cx, |reveal, _| {
                        reveal.scroll_handle = scroll_handle;
                        reveal.bounds = Some(bounds);
                    })
                },
                |_, _, _, _| {},
            )
            .absolute()
            .inset_0(),
        )
    }
}

/// Builder methods for components carrying [`FocusProps`].
pub trait WithFocus: Sized {
    #[doc(hidden)]
    fn focus_props(&mut self) -> &mut FocusProps;

    /// Focus this element when it is first created.
    fn auto_focus(mut self) -> Self {
        self.focus_props().auto_focus = true;
        self
    }

    /// Include (default) or exclude this element from the tab order.
    fn tab_stop(mut self, tab_stop: bool) -> Self {
        self.focus_props().tab_stop = tab_stop;
        self
    }

    /// Position of this element in the tab order.
    fn tab_index(mut self, tab_index: isize) -> Self {
        self.focus_props().tab_index = tab_index;
        self
    }

    /// Scroll this element into view inside the container tracked by `handle`
    /// when it gains keyboard focus.
    fn reveal_on_focus(mut self, handle: &ScrollHandle) -> Self {
        self.focus_props().reveal_in = Some(handle.clone());
        self
    }
}

/// Scrolls an element into view when it gains keyboard focus.
struct Reveal {
    scroll_handle: ScrollHandle,
    bounds: Option<Bounds<Pixels>>,
    _subscription: Subscription,
}

impl Reveal {
    fn new(
        focus_handle: &FocusHandle,
        scroll_handle: ScrollHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let _subscription = cx.on_focus_in(focus_handle, window, |_, window, cx| {
            cx.defer_in(window, |this, window, _| {
                if window.last_input_was_keyboard() {
                    this.scroll_into_view(window);
                }
            });
        });

        Self {
            scroll_handle,
            bounds: None,
            _subscription,
        }
    }

    fn scroll_into_view(&self, window: &mut Window) {
        let Some(bounds) = self.bounds else {
            return;
        };

        let viewport = self.scroll_handle.bounds();
        let max_offset = self.scroll_handle.max_offset();
        let offset = self.scroll_handle.offset();

        let revealed = point(
            reveal_axis(
                offset.x,
                bounds.left()..bounds.right(),
                viewport.left()..viewport.right(),
                max_offset.x,
            ),
            reveal_axis(
                offset.y,
                bounds.top()..bounds.bottom(),
                viewport.top()..viewport.bottom(),
                max_offset.y,
            ),
        );

        if revealed != offset {
            self.scroll_handle.set_offset(revealed);
            window.refresh();
        }
    }
}

/// Minimal scroll delta that brings `element` into `viewport` along one axis,
/// clamped to the scrollable range.
fn reveal_axis(
    offset: Pixels,
    element: Range<Pixels>,
    viewport: Range<Pixels>,
    max_offset: Pixels,
) -> Pixels {
    let delta = if element.start < viewport.start
        || element.end - element.start > viewport.end - viewport.start
    {
        viewport.start - element.start
    } else if element.end > viewport.end {
        viewport.end - element.end
    } else {
        return offset;
    };

    (offset + delta).clamp(-max_offset, px(0.))
}
