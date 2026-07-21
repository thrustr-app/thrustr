use gpui::{
    Along, Anchor, App, BorderStyle, Bounds, ContentMask, Context, Corners, CursorStyle,
    DispatchPhase, Div, Edges, Element, ElementId, Entity, GlobalElementId, Hitbox, HitboxBehavior,
    Hsla, InteractiveElement, Interactivity, IntoElement, IsZero, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Point, Position, Render,
    RenderOnce, ScrollHandle, ScrollWheelEvent, Stateful, StatefulInteractiveElement, Style,
    StyleRefinement, Styled, UniformList, UniformListDecoration, UniformListScrollHandle, Window,
    px, quad, relative, size,
};
use smallvec::SmallVec;
use std::{
    ops::Range,
    time::{Duration, Instant},
};
use theme::ThemeExt as _;

pub const SCROLLBAR_WIDTH: Pixels = px(12.);

const THUMB_INSET: Pixels = px(3.);
const MIN_THUMB_SIZE: Pixels = px(25.);
const HIDE_DELAY: Duration = Duration::from_millis(1200);
const FADE_DURATION: Duration = Duration::from_millis(400);

pub use gpui::Axis as ScrollbarAxis;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollAxes {
    Horizontal,
    Vertical,
    Both,
}

impl ScrollAxes {
    fn contains(self, axis: ScrollbarAxis) -> bool {
        matches!(
            (self, axis),
            (Self::Both, _)
                | (Self::Horizontal, ScrollbarAxis::Horizontal)
                | (Self::Vertical, ScrollbarAxis::Vertical)
        )
    }
}

/// Handle to whichever kind of container the scrollbar drives.
#[derive(Clone)]
enum ScrollbarTarget {
    Div(ScrollHandle),
    UniformList(UniformListScrollHandle),
}

impl ScrollbarTarget {
    fn base_handle(&self) -> ScrollHandle {
        match self {
            Self::Div(handle) => handle.clone(),
            Self::UniformList(handle) => handle.0.borrow().base_handle.clone(),
        }
    }

    fn max_offset(&self) -> Point<Pixels> {
        self.base_handle().max_offset()
    }

    fn set_offset(&self, offset: Point<Pixels>) {
        self.base_handle().set_offset(offset);
    }

    fn offset(&self) -> Point<Pixels> {
        self.base_handle().offset()
    }

    fn viewport(&self) -> Bounds<Pixels> {
        self.base_handle().bounds()
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
enum ThumbState {
    #[default]
    Inactive,
    Hover(ScrollbarAxis),
    Dragging(ScrollbarAxis, Pixels),
}

impl ThumbState {
    fn is_dragging(&self) -> bool {
        matches!(self, Self::Dragging(..))
    }

    fn is_active(&self) -> bool {
        !matches!(self, Self::Inactive)
    }
}

/// Persistent scrollbar state held in an `Entity`.
pub struct ScrollbarState {
    handle: ScrollbarTarget,
    /// Set only when the scrollbar created the handle itself, in which case the
    /// container needs `track_scroll` wired to it.
    owned_handle: Option<ScrollHandle>,
    axes: ScrollAxes,
    thumb: ThumbState,
    last_activity: Option<Instant>,
    last_scroll: Option<Instant>,
    last_layout: Option<LayoutState>,
}

impl ScrollbarState {
    fn owned(axes: ScrollAxes) -> Self {
        let handle = ScrollHandle::new();
        Self {
            handle: ScrollbarTarget::Div(handle.clone()),
            owned_handle: Some(handle),
            axes,
            thumb: ThumbState::default(),
            last_activity: None,
            last_scroll: None,
            last_layout: None,
        }
    }

    fn borrowed(handle: ScrollbarTarget, axes: ScrollAxes) -> Self {
        Self {
            handle,
            owned_handle: None,
            axes,
            thumb: ThumbState::default(),
            last_activity: None,
            last_scroll: None,
            last_layout: None,
        }
    }

    pub fn is_dragging(&self) -> bool {
        self.thumb.is_dragging()
    }

    /// Opacity for overlays keyed to actual scrolling.
    pub fn scroll_opacity(&self) -> f32 {
        if self.is_dragging() {
            return 1.;
        }
        fade_from(self.last_scroll)
    }

    /// Center of the thumb along `axis`, measured from the start of the scroll
    /// container. Returns `None` when there is nothing to scroll on that axis.
    ///
    /// Useful for positioning overlays relative to the thumb. Computing the
    /// position from the scroll fraction can be slightly inaccurate near the ends
    /// of the track because the thumb doesn't travel its full length.
    pub fn thumb_center(&self, axis: ScrollbarAxis) -> Option<Pixels> {
        let layout = self.last_layout.as_ref()?;
        let bar = layout.bars.iter().find(|bar| bar.axis == axis)?;
        Some(bar.thumb_bounds.center().along(axis) - layout.parent_hitbox.bounds.origin.along(axis))
    }

    fn mark_active(&mut self, cx: &mut Context<Self>) {
        self.last_activity = Some(Instant::now());
        cx.notify();
    }

    fn set_thumb(&mut self, thumb: ThumbState, cx: &mut Context<Self>) {
        if self.thumb != thumb {
            self.thumb = thumb;
            self.mark_active(cx);
        }
    }

    fn set_offset(&mut self, offset: Point<Pixels>, cx: &mut Context<Self>) {
        self.handle.set_offset(offset);
        self.mark_scrolled(cx);
    }

    fn mark_scrolled(&mut self, cx: &mut Context<Self>) {
        self.last_scroll = Some(Instant::now());
        self.mark_active(cx);
    }

    /// Current opacity of the bar.
    fn opacity(&self, container_hovered: bool) -> f32 {
        if container_hovered || self.thumb.is_active() {
            return 1.;
        }
        fade_from(self.last_activity)
    }

    fn layout_for(&self, axis: ScrollbarAxis) -> Option<&ScrollbarLayout> {
        self.last_layout
            .as_ref()?
            .bars
            .iter()
            .find(|bar| bar.axis == axis)
    }

    fn hit(&self, position: &Point<Pixels>) -> Option<&ScrollbarLayout> {
        self.last_layout
            .as_ref()?
            .bars
            .iter()
            .find(|bar| bar.track_bounds.contains(position))
    }

    fn parent_hovered(&self, window: &Window) -> bool {
        self.last_layout
            .as_ref()
            .is_some_and(|layout| layout.parent_hitbox.is_hovered(window))
    }

    fn update_hover(&mut self, position: &Point<Pixels>, cx: &mut Context<Self>) {
        let hovered = self
            .last_layout
            .as_ref()
            .and_then(|layout| {
                layout
                    .bars
                    .iter()
                    .find(|bar| bar.thumb_bounds.contains(position))
            })
            .map(|bar| bar.axis);

        let thumb = match hovered {
            Some(axis) => ThumbState::Hover(axis),
            None => ThumbState::Inactive,
        };
        self.set_thumb(thumb, cx);
    }

    /// Axes that currently have something to scroll.
    fn scrollable_axes(&self) -> impl Iterator<Item = ScrollbarAxis> + '_ {
        let max_offset = self.handle.max_offset();
        let viewport = self.handle.viewport().size;

        [ScrollbarAxis::Horizontal, ScrollbarAxis::Vertical]
            .into_iter()
            .filter(move |&axis| self.axes.contains(axis))
            .filter(move |&axis| {
                !max_offset.along(axis).is_zero() && !viewport.along(axis).is_zero()
            })
    }
}

/// Fade curve used by the scrollbar and scroll-driven overlays. Stays fully
/// opaque for [`HIDE_DELAY`] after `since`, then fades out over
/// [`FADE_DURATION`].
fn fade_from(since: Option<Instant>) -> f32 {
    let Some(since) = since else {
        return 0.;
    };
    let elapsed = since.elapsed();
    if elapsed < HIDE_DELAY {
        return 1.;
    }
    let faded = (elapsed - HIDE_DELAY).as_secs_f32() / FADE_DURATION.as_secs_f32();
    (1. - faded).clamp(0., 1.)
}

/// Where the thumb sits within its track, in pixels along the scroll axis.
fn thumb_placement(
    track: Pixels,
    viewport: Pixels,
    max_offset: Pixels,
    offset: Pixels,
) -> Option<(Pixels, Pixels)> {
    if track <= Pixels::ZERO || viewport <= Pixels::ZERO || max_offset <= Pixels::ZERO {
        return None;
    }

    let content = viewport + max_offset;
    let size = MIN_THUMB_SIZE.max(track * (viewport / content));
    if size >= track {
        return None;
    }

    let scrolled = offset.clamp(-max_offset, Pixels::ZERO).abs();
    let start = (scrolled / max_offset) * (track - size);
    Some((start, size))
}

/// Inverse of [`thumb_placement`]: the offset that puts the thumb's leading
/// edge at `thumb_start`.
fn offset_for_thumb_start(
    track: Pixels,
    thumb_size: Pixels,
    thumb_start: Pixels,
    max_offset: Pixels,
) -> Pixels {
    let travel = track - thumb_size;
    if travel <= Pixels::ZERO {
        return Pixels::ZERO;
    }
    -max_offset * (thumb_start.clamp(Pixels::ZERO, travel) / travel)
}

impl Render for ScrollbarState {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        ScrollbarElement { state: cx.entity() }
    }
}

pub struct ListScrollbar(Entity<ScrollbarState>);

impl ListScrollbar {
    pub fn new(state: Entity<ScrollbarState>) -> Self {
        Self(state)
    }
}

impl UniformListDecoration for ListScrollbar {
    fn compute(
        &self,
        _visible_range: Range<usize>,
        _bounds: Bounds<Pixels>,
        _scroll_offset: Point<Pixels>,
        _item_height: Pixels,
        _item_count: usize,
        _window: &mut Window,
        _cx: &mut App,
    ) -> gpui::AnyElement {
        ScrollbarElement {
            state: self.0.clone(),
        }
        .into_any()
    }
}

enum MouseTarget {
    TrackClick,
    ThumbDrag(Pixels),
}

struct ScrollbarLayout {
    axis: ScrollbarAxis,
    track_bounds: Bounds<Pixels>,
    thumb_bounds: Bounds<Pixels>,
    /// The visibly narrower rect actually painted.
    fill_bounds: Bounds<Pixels>,
    hitbox: Hitbox,
}

impl ScrollbarLayout {
    /// Inverse of the thumb placement. Maps a pointer position back to the
    /// scroll offset that would put the thumb there.
    fn offset_for(
        &self,
        position: Point<Pixels>,
        max_offset: Point<Pixels>,
        target: MouseTarget,
    ) -> Pixels {
        let axis = self.axis;
        let thumb_size = self.thumb_bounds.size.along(axis);

        let grab_offset = match target {
            MouseTarget::TrackClick => thumb_size / 2.,
            MouseTarget::ThumbDrag(offset) => offset,
        };

        let thumb_start = position.along(axis) - self.track_bounds.origin.along(axis) - grab_offset;

        offset_for_thumb_start(
            self.track_bounds.size.along(axis),
            thumb_size,
            thumb_start,
            max_offset.along(axis),
        )
    }
}

struct LayoutState {
    parent_hitbox: Hitbox,
    bars: SmallVec<[ScrollbarLayout; 2]>,
}

struct ScrollbarElement {
    state: Entity<ScrollbarState>,
}

impl Element for ScrollbarElement {
    type RequestLayoutState = ();
    type PrepaintState = Option<LayoutState>;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let style = Style {
            position: Position::Absolute,
            inset: Edges::default(),
            size: size(relative(1.), relative(1.)).map(Into::into),
            ..Default::default()
        };

        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let state = self.state.read(cx);
        let axes = state.scrollable_axes().collect::<SmallVec<[_; 2]>>();
        let max_offset = state.handle.max_offset();
        let offset = state.handle.offset();

        let viewport_bounds = state.handle.viewport();
        let viewport = viewport_bounds.size;

        // Shorten each track when both bars are present so they do not fight
        // over the shared corner.
        let corner_gap = if axes.len() == 2 {
            SCROLLBAR_WIDTH
        } else {
            Pixels::ZERO
        };

        let bars: SmallVec<[ScrollbarLayout; 2]> = axes
            .into_iter()
            .filter_map(|axis| {
                let anchor = match axis {
                    ScrollbarAxis::Horizontal => Anchor::BottomLeft,
                    ScrollbarAxis::Vertical => Anchor::TopRight,
                };

                let track_bounds = Bounds::from_anchor_and_size(
                    anchor,
                    viewport_bounds.corner(anchor),
                    viewport_bounds
                        .size
                        .apply_along(axis.invert(), |_| SCROLLBAR_WIDTH)
                        .apply_along(axis, |length| length - corner_gap),
                );

                let track = track_bounds.size.along(axis);
                let (start, thumb_size) = thumb_placement(
                    track,
                    viewport.along(axis),
                    max_offset.along(axis),
                    offset.along(axis),
                )?;

                let thumb_bounds = Bounds::new(
                    track_bounds
                        .origin
                        .apply_along(axis, |origin| origin + start),
                    track_bounds.size.apply_along(axis, |_| thumb_size),
                );

                let fill_bounds = Bounds::new(
                    thumb_bounds
                        .origin
                        .apply_along(axis.invert(), |origin| origin + THUMB_INSET),
                    thumb_bounds
                        .size
                        .apply_along(axis.invert(), |size| (size - THUMB_INSET * 2.).max(px(1.))),
                );

                Some(ScrollbarLayout {
                    axis,
                    track_bounds,
                    thumb_bounds,
                    fill_bounds,
                    // Grabbing anywhere on the track works, so the hitbox
                    // covers the track rather than just the painted thumb.
                    hitbox: window
                        .insert_hitbox(track_bounds, HitboxBehavior::BlockMouseExceptScroll),
                })
            })
            .collect();

        Some(LayoutState {
            parent_hitbox: window.insert_hitbox(viewport_bounds, HitboxBehavior::Normal),
            bars,
        })
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint_state: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(layout) = prepaint_state.take() else {
            return;
        };

        let hovered = layout.parent_hitbox.is_hovered(window);
        // Clip to the scroll container, matching the track, rather than to this
        // element's own bounds which are translated by the scroll offset.
        let bounds = layout.parent_hitbox.bounds;
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            let opacity = self.state.read(cx).opacity(hovered);
            if opacity > 0. {
                let theme = cx.theme();
                let state = self.state.read(cx);

                for bar in &layout.bars {
                    let base = match state.thumb {
                        ThumbState::Dragging(axis, _) if axis == bar.axis => theme.colors.primary,
                        ThumbState::Hover(axis) if axis == bar.axis => theme.colors.primary,
                        _ => theme.colors.secondary,
                    };

                    let mut color = base;
                    color.a *= opacity;

                    window.paint_quad(quad(
                        bar.fill_bounds,
                        Corners::all(Pixels::MAX).clamp_radii_for_quad_size(bar.fill_bounds.size),
                        color,
                        Edges::default(),
                        Hsla::transparent_black(),
                        BorderStyle::default(),
                    ));

                    if state.thumb.is_dragging() {
                        window.set_window_cursor_style(CursorStyle::Arrow);
                    } else {
                        window.set_cursor_style(CursorStyle::Arrow, &bar.hitbox);
                    }
                }

                // Keep painting while a fade is pending.
                let fade_pending = opacity < 1.
                    || state.scroll_opacity() > 0.
                    || !(hovered || state.thumb.is_active());
                if fade_pending {
                    window.request_animation_frame();
                }
            }

            // While dragging we must see moves before anything else can claim them.
            let capture_phase = if self.state.read(cx).thumb.is_dragging() {
                DispatchPhase::Capture
            } else {
                DispatchPhase::Bubble
            };

            self.state
                .update(cx, |state, _| state.last_layout = Some(layout));

            window.on_mouse_event({
                let state = self.state.clone();
                move |event: &MouseDownEvent, phase, _window, cx| {
                    if phase != capture_phase || event.button != MouseButton::Left {
                        return;
                    }

                    state.update(cx, |state, cx| {
                        let Some(bar) = state.hit(&event.position) else {
                            return;
                        };
                        let axis = bar.axis;

                        let grab = if bar.thumb_bounds.contains(&event.position) {
                            event.position.along(axis) - bar.thumb_bounds.origin.along(axis)
                        } else {
                            // Click-to-jump centers the thumb under the
                            // pointer, and the same press keeps dragging it
                            // from there without needing a second grab.
                            let offset = bar.offset_for(
                                event.position,
                                state.handle.max_offset(),
                                MouseTarget::TrackClick,
                            );
                            let thumb_size = bar.thumb_bounds.size.along(axis);
                            let offset = state.handle.offset().apply_along(axis, |_| offset);
                            state.set_offset(offset, cx);
                            thumb_size / 2.
                        };

                        state.thumb = ThumbState::Dragging(axis, grab);
                        state.mark_active(cx);
                        cx.stop_propagation();
                    });
                }
            });

            window.on_mouse_event({
                let state = self.state.clone();
                move |event: &MouseMoveEvent, phase, window, cx| {
                    if phase != capture_phase {
                        return;
                    }

                    let dragging = match state.read(cx).thumb {
                        ThumbState::Dragging(axis, grab) if event.dragging() => Some((axis, grab)),
                        _ => None,
                    };

                    state.update(cx, |state, cx| match dragging {
                        Some((axis, grab)) => {
                            let Some(bar) = state.layout_for(axis) else {
                                return;
                            };
                            let offset = bar.offset_for(
                                event.position,
                                state.handle.max_offset(),
                                MouseTarget::ThumbDrag(grab),
                            );
                            let offset = state.handle.offset().apply_along(axis, |_| offset);
                            state.set_offset(offset, cx);
                            cx.stop_propagation();
                        }
                        None if event.pressed_button.is_none() => {
                            if state.parent_hovered(window) {
                                state.mark_active(cx);
                                state.update_hover(&event.position, cx);
                            } else {
                                state.set_thumb(ThumbState::Inactive, cx);
                            }
                        }
                        None => {}
                    });
                }
            });

            window.on_mouse_event({
                let state = self.state.clone();
                move |event: &MouseUpEvent, phase, _window, cx| {
                    if phase != capture_phase {
                        return;
                    }
                    state.update(cx, |state, cx| {
                        state.update_hover(&event.position, cx);
                    });
                }
            });

            // Wheel scrolling should reveal the bar even though the pointer
            // never touches it.
            window.on_mouse_event({
                let state = self.state.clone();
                move |_: &ScrollWheelEvent, phase, window, cx| {
                    if phase.bubble() {
                        state.update(cx, |state, cx| {
                            if state.parent_hovered(window) {
                                state.mark_scrolled(cx);
                            }
                        });
                    }
                }
            });
        });
    }
}

impl IntoElement for ScrollbarElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

/// A `div` with an overlaid scrollbar.
#[derive(IntoElement)]
pub struct Scrollable {
    div: Stateful<Div>,
    axes: ScrollAxes,
    id: ElementId,
    handle: Option<ScrollHandle>,
}

impl Scrollable {
    fn new(div: Stateful<Div>, axes: ScrollAxes, id: ElementId) -> Self {
        Self {
            div,
            axes,
            id,
            handle: None,
        }
    }

    /// Drive the scrollbar from a caller-owned handle instead of an internally
    /// created one.
    pub fn handle(mut self, handle: &ScrollHandle) -> Self {
        self.handle = Some(handle.clone());
        self
    }
}

impl Styled for Scrollable {
    fn style(&mut self) -> &mut StyleRefinement {
        self.div.style()
    }
}

impl ParentElement for Scrollable {
    fn extend(&mut self, elements: impl IntoIterator<Item = gpui::AnyElement>) {
        self.div.extend(elements);
    }
}

impl InteractiveElement for Scrollable {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.div.interactivity()
    }
}

impl StatefulInteractiveElement for Scrollable {}

impl RenderOnce for Scrollable {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            div,
            axes,
            id,
            handle,
        } = self;

        let state = window.use_keyed_state(id, cx, |_, _| match &handle {
            Some(handle) => ScrollbarState::borrowed(ScrollbarTarget::Div(handle.clone()), axes),
            None => ScrollbarState::owned(axes),
        });
        state.update(cx, |state, _| {
            state.axes = axes;
            // The caller may have swapped handles between frames.
            if let Some(handle) = &handle {
                state.handle = ScrollbarTarget::Div(handle.clone());
            }
        });

        let handle = handle
            .or_else(|| state.read(cx).owned_handle.clone())
            .expect("a div scrollbar either borrows a caller handle or owns one it created");

        let div = match axes {
            ScrollAxes::Horizontal => div.overflow_x_scroll(),
            ScrollAxes::Vertical => div.overflow_y_scroll(),
            ScrollAxes::Both => div.overflow_scroll(),
        };

        div.track_scroll(&handle).child(state)
    }
}

/// Adds an overlaid scrollbar to a `div`.
pub trait WithScrollbar: Sized {
    #[track_caller]
    fn scrollbars(self, axes: ScrollAxes) -> Scrollable;

    #[track_caller]
    fn overflow_scrollbar(self) -> Scrollable {
        self.scrollbars(ScrollAxes::Both)
    }

    #[track_caller]
    fn overflow_x_scrollbar(self) -> Scrollable {
        self.scrollbars(ScrollAxes::Horizontal)
    }

    #[track_caller]
    fn overflow_y_scrollbar(self) -> Scrollable {
        self.scrollbars(ScrollAxes::Vertical)
    }
}

impl WithScrollbar for Div {
    #[track_caller]
    fn scrollbars(self, axes: ScrollAxes) -> Scrollable {
        let id = caller_id();
        Scrollable::new(self.id(id.clone()), axes, id)
    }
}

impl WithScrollbar for Stateful<Div> {
    #[track_caller]
    fn scrollbars(self, axes: ScrollAxes) -> Scrollable {
        Scrollable::new(self, axes, caller_id())
    }
}

/// Adds an overlaid scrollbar to a `uniform_list`.
pub trait WithListScrollbar: Sized {
    #[track_caller]
    fn vertical_scrollbar(
        self,
        handle: &UniformListScrollHandle,
        window: &mut Window,
        cx: &mut App,
    ) -> Self;
}

impl WithListScrollbar for UniformList {
    #[track_caller]
    fn vertical_scrollbar(
        self,
        handle: &UniformListScrollHandle,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let state = list_scrollbar_state(caller_id(), handle, window, cx);
        self.with_decoration(ListScrollbar(state))
    }
}

/// Resolves the persistent scrollbar state for a caller-owned handle.
pub fn list_scrollbar_state(
    id: impl Into<ElementId>,
    handle: &UniformListScrollHandle,
    window: &mut Window,
    cx: &mut App,
) -> Entity<ScrollbarState> {
    let state = window.use_keyed_state(id.into(), cx, {
        let handle = handle.clone();
        move |_, _| {
            ScrollbarState::borrowed(ScrollbarTarget::UniformList(handle), ScrollAxes::Vertical)
        }
    });

    // The caller may have swapped handles between frames.
    state.update(cx, |state, _| {
        state.handle = ScrollbarTarget::UniformList(handle.clone())
    });
    state
}

#[track_caller]
fn caller_id() -> ElementId {
    std::panic::Location::caller().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEWPORT: Pixels = px(900.);
    const MAX_OFFSET: Pixels = px(206_639.);

    /// Dragging must be idempotent. Holding the thumb still re-runs this
    /// mapping every frame against the thumb it produced last frame, so if a
    /// round trip does not land back on the same offset the content oscillates
    /// for as long as the button is held.
    #[test]
    fn thumb_placement_round_trips() {
        let track = VIEWPORT;

        for step in 0..=100 {
            let offset = -MAX_OFFSET * (step as f32 / 100.);
            let (start, size) =
                thumb_placement(track, VIEWPORT, MAX_OFFSET, offset).expect("thumb should exist");

            let recovered = offset_for_thumb_start(track, size, start, MAX_OFFSET);

            assert!(
                (recovered - offset).abs() < px(0.5),
                "offset {offset:?} placed thumb at {start:?} but mapped back to {recovered:?}",
            );
        }
    }

    #[test]
    fn thumb_placement_round_trips_when_track_differs_from_viewport() {
        // Both bars visible, so the vertical track is shortened by the corner.
        let track = VIEWPORT - SCROLLBAR_WIDTH;

        for step in 0..=100 {
            let offset = -MAX_OFFSET * (step as f32 / 100.);
            let (start, size) =
                thumb_placement(track, VIEWPORT, MAX_OFFSET, offset).expect("thumb should exist");

            assert!(
                start + size <= track + px(0.5),
                "thumb {start:?}+{size:?} overflows track {track:?}",
            );

            let recovered = offset_for_thumb_start(track, size, start, MAX_OFFSET);

            assert!(
                (recovered - offset).abs() < px(0.5),
                "offset {offset:?} placed thumb at {start:?} but mapped back to {recovered:?}",
            );
        }
    }

    #[test]
    fn thumb_is_hidden_when_there_is_nothing_to_scroll() {
        assert!(thumb_placement(px(900.), px(900.), Pixels::ZERO, Pixels::ZERO).is_none());
        assert!(thumb_placement(Pixels::ZERO, px(900.), px(100.), Pixels::ZERO).is_none());
        // Track too short to hold.
        assert!(thumb_placement(px(10.), px(900.), px(100.), Pixels::ZERO).is_none());
    }

    #[test]
    fn thumb_never_shrinks_below_the_minimum() {
        let (_, size) = thumb_placement(px(900.), px(900.), px(1_000_000.), Pixels::ZERO).unwrap();
        assert_eq!(size, MIN_THUMB_SIZE);
    }
}
