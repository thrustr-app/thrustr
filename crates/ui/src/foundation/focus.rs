use gpui::{
    App, Bounds, Context, ElementId, FocusHandle, IsZero, ParentElement, Pixels, ScrollHandle,
    Styled, Subscription, Window, canvas, point, px,
};
use std::{cell::Cell, ops::Range, rc::Rc};

/// Keyboard-focus behavior shared by focusable components.
pub struct FocusProps {
    pub(crate) auto_focus: bool,
    pub(crate) tab_index: isize,
    pub(crate) tab_stop: bool,
    reveal: Option<Reveal>,
}

impl Default for FocusProps {
    fn default() -> Self {
        Self {
            auto_focus: false,
            tab_index: 0,
            tab_stop: true,
            reveal: None,
        }
    }
}

impl FocusProps {
    /// Apply the configured tab order to `handle`.
    pub(crate) fn configure(&self, mut handle: FocusHandle) -> FocusHandle {
        if handle.tab_stop != self.tab_stop {
            handle = handle.tab_stop(self.tab_stop);
        }
        if handle.tab_index != self.tab_index {
            handle = handle.tab_index(self.tab_index);
        }
        handle
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
        let Some(reveal) = &self.reveal else {
            return element;
        };

        let listener = window.use_keyed_state(key.into(), cx, {
            let focus_handle = focus_handle.clone();
            move |window, cx| RevealListener::new(&focus_handle, window, cx)
        });

        listener.update(cx, {
            let reveal = reveal.clone();
            move |listener, _| listener.reveal = Some(reveal)
        });

        let bounds = reveal.bounds.clone();
        element.child(
            canvas(
                move |element_bounds, _, _| bounds.set(element_bounds),
                |_, _, _, _| {},
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
    }
}

/// Builder methods for components carrying [`FocusProps`].
pub trait WithFocus: Sized {
    #[doc(hidden)]
    fn focus_props(&mut self) -> &mut FocusProps;

    /// Focus this element when it is first created.
    fn auto_focus(mut self, auto_focus: bool) -> Self {
        self.focus_props().auto_focus = auto_focus;
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
        self.focus_props().reveal = Some(Reveal::new(handle));
        self
    }
}

/// Scrolls an element into view when it gains keyboard focus.
#[derive(Clone)]
struct Reveal {
    handle: ScrollHandle,
    bounds: Rc<Cell<Bounds<Pixels>>>,
}

impl Reveal {
    fn new(handle: &ScrollHandle) -> Self {
        Self {
            handle: handle.clone(),
            bounds: Rc::default(),
        }
    }

    fn scroll_into_view(&self, window: &mut Window) {
        let bounds = self.bounds.get();
        if bounds.size.width.is_zero() && bounds.size.height.is_zero() {
            // Not painted yet, so there is nothing to reveal.
            return;
        }

        let viewport = self.handle.bounds();
        let max_offset = self.handle.max_offset();
        let offset = self.handle.offset();

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
            self.handle.set_offset(revealed);
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

struct RevealListener {
    reveal: Option<Reveal>,
    _subscription: Subscription,
}

impl RevealListener {
    fn new(focus_handle: &FocusHandle, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let _subscription = cx.on_focus_in(focus_handle, window, |this, window, _| {
            if window.last_input_was_keyboard()
                && let Some(reveal) = &this.reveal
            {
                reveal.scroll_into_view(window);
            }
        });

        Self {
            reveal: None,
            _subscription,
        }
    }
}
