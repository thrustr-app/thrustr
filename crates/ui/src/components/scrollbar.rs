use gpui::{
    Along, Anchor, AnyElement, App, BorderStyle, Bounds, ContentMask, Context, Corners,
    CursorStyle, DispatchPhase, Div, Edges, Element, ElementId, Entity, GlobalElementId, Hitbox,
    HitboxBehavior, Hsla, InspectorElementId, InteractiveElement, Interactivity, IntoElement,
    IsZero, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement,
    Pixels, Point, Position, Render, RenderOnce, ScrollHandle, ScrollWheelEvent, Stateful,
    StatefulInteractiveElement, Style, StyleRefinement, Styled, Task, UniformListDecoration,
    UniformListScrollHandle, Window, px, quad, relative, size,
};
use smallvec::SmallVec;
use std::{
    ops::Range,
    time::{Duration, Instant},
};
use theme::ThemeExt;

pub use gpui::Axis as ScrollbarAxis;

pub const SCROLLBAR_WIDTH: Pixels = px(12.);

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
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
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
            Some(handle) => ScrollbarState::borrowed(handle.clone(), axes),
            None => ScrollbarState::owned(axes),
        });
        state.update(cx, |state, _| {
            state.axes = axes;
            // The caller may have swapped handles between frames.
            if let Some(handle) = &handle {
                state.handle = handle.clone();
            }
        });

        let handle = handle
            .or_else(|| state.read(cx).owned_handle.clone())
            .expect("a div scrollbar should borrow a caller handle or own one it created");

        let div = match axes {
            ScrollAxes::Horizontal => div.overflow_x_scroll(),
            ScrollAxes::Vertical => div.overflow_y_scroll(),
            ScrollAxes::Both => div.overflow_scroll(),
        };

        div.track_scroll(&handle).child(state)
    }
}

/// Adds a scrollbar to a [`Div`].
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
    ) -> AnyElement {
        ScrollbarElement {
            state: self.0.clone(),
        }
        .into_any()
    }
}

const THUMB_INSET: Pixels = px(3.);
const MIN_THUMB_SIZE: Pixels = px(25.);
const HIDE_DELAY: Duration = Duration::from_millis(1200);
const FADE_DURATION: Duration = Duration::from_millis(400);

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

#[derive(Debug, Default, Clone, Copy)]
struct FadeTimer(Option<Instant>);

impl FadeTimer {
    fn mark(&mut self) {
        self.0 = Some(Instant::now());
    }

    fn since(&self) -> Option<Instant> {
        self.0
    }

    fn opacity(&self) -> f32 {
        fade_from(self.0)
    }
}

pub struct ScrollbarState {
    handle: ScrollHandle,
    /// Set only when the scrollbar created the handle itself, in which case the
    /// container needs `track_scroll` wired to it.
    owned_handle: Option<ScrollHandle>,
    axes: ScrollAxes,
    thumb: ThumbState,
    activity: FadeTimer,
    scroll: FadeTimer,
    last_layout: Option<LayoutState>,
    was_hovered: bool,
    fade_wake: Option<(Instant, Task<()>)>,
}

impl ScrollbarState {
    fn new(handle: ScrollHandle, owned_handle: Option<ScrollHandle>, axes: ScrollAxes) -> Self {
        Self {
            handle,
            owned_handle,
            axes,
            thumb: ThumbState::default(),
            activity: FadeTimer::default(),
            scroll: FadeTimer::default(),
            last_layout: None,
            was_hovered: false,
            fade_wake: None,
        }
    }

    fn owned(axes: ScrollAxes) -> Self {
        let handle = ScrollHandle::new();
        Self::new(handle.clone(), Some(handle), axes)
    }

    fn borrowed(handle: ScrollHandle, axes: ScrollAxes) -> Self {
        Self::new(handle, None, axes)
    }

    pub fn for_uniform_list(handle: &UniformListScrollHandle) -> Self {
        let handle = handle.0.borrow().base_handle.clone();
        Self::borrowed(handle, ScrollAxes::Vertical)
    }

    pub fn is_dragging(&self) -> bool {
        self.thumb.is_dragging()
    }

    pub fn flash(&mut self, cx: &mut Context<Self>) {
        self.mark_scrolled(cx);
    }

    pub fn scroll_opacity(&self) -> f32 {
        if self.is_dragging() {
            return 1.;
        }
        self.scroll.opacity()
    }

    pub fn thumb_center(&self, axis: ScrollbarAxis) -> Option<Pixels> {
        let layout = self.last_layout.as_ref()?;
        let bar = layout.bars.iter().find(|bar| bar.axis == axis)?;
        Some(bar.thumb_bounds.center().along(axis) - layout.parent_hitbox.bounds.origin.along(axis))
    }

    fn mark_active(&mut self, cx: &mut Context<Self>) {
        self.activity.mark();
        cx.notify();
    }

    fn set_thumb(&mut self, thumb: ThumbState, cx: &mut Context<Self>) {
        if self.thumb != thumb {
            self.thumb = thumb;
            self.mark_active(cx);
        }
    }

    fn set_offset(&mut self, offset: Point<Pixels>, cx: &mut Context<Self>) {
        if self.handle.offset() == offset {
            self.scroll.mark();
            return;
        }
        self.handle.set_offset(offset);
        self.mark_scrolled(cx);
    }

    fn mark_scrolled(&mut self, cx: &mut Context<Self>) {
        self.scroll.mark();
        self.mark_active(cx);
    }

    fn schedule_fade(&mut self, cx: &mut Context<Self>) {
        let now = Instant::now();
        if self.fade_wake.as_ref().is_some_and(|(at, _)| *at > now) {
            return;
        }
        let Some(since) = self.activity.since().max(self.scroll.since()) else {
            return;
        };

        let deadline = since + HIDE_DELAY;
        let Some(delay) = deadline.checked_duration_since(now) else {
            return;
        };

        let task = cx.spawn(async move |state, cx| {
            cx.background_executor().timer(delay).await;
            state.update(cx, |_, cx| cx.notify()).ok();
        });
        self.fade_wake = Some((deadline, task));
    }

    fn needs_fade_wakeup(&self, container_hovered: bool) -> bool {
        let scroll_overlay_will_fade = self.scroll_opacity() >= 1.;
        let bar_will_fade =
            !container_hovered && !self.thumb.is_active() && self.opacity(container_hovered) >= 1.;
        scroll_overlay_will_fade || bar_will_fade
    }

    fn opacity(&self, container_hovered: bool) -> f32 {
        if container_hovered || self.thumb.is_active() {
            return 1.;
        }
        self.activity.opacity()
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

    fn scrollable_axes(&self) -> impl Iterator<Item = ScrollbarAxis> + '_ {
        let max_offset = self.handle.max_offset();
        let viewport = self.handle.bounds().size;

        [ScrollbarAxis::Horizontal, ScrollbarAxis::Vertical]
            .into_iter()
            .filter(move |&axis| self.axes.contains(axis))
            .filter(move |&axis| {
                !max_offset.along(axis).is_zero() && !viewport.along(axis).is_zero()
            })
    }

    fn handle_mouse_down(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        let Some(bar) = self.hit(&event.position) else {
            return;
        };
        let axis = bar.axis;

        let grab = if bar.thumb_bounds.contains(&event.position) {
            event.position.along(axis) - bar.thumb_bounds.origin.along(axis)
        } else {
            let offset = bar.offset_for(
                event.position,
                self.handle.max_offset(),
                MouseTarget::TrackClick,
            );
            let thumb_size = bar.thumb_bounds.size.along(axis);
            let offset = self.handle.offset().apply_along(axis, |_| offset);
            self.set_offset(offset, cx);
            thumb_size / 2.
        };

        self.thumb = ThumbState::Dragging(axis, grab);
        self.mark_active(cx);
        cx.stop_propagation();
    }

    fn handle_drag(
        &mut self,
        event: &MouseMoveEvent,
        axis: ScrollbarAxis,
        grab: Pixels,
        cx: &mut Context<Self>,
    ) {
        let Some(bar) = self.layout_for(axis) else {
            return;
        };
        let offset = bar.offset_for(
            event.position,
            self.handle.max_offset(),
            MouseTarget::ThumbDrag(grab),
        );
        let offset = self.handle.offset().apply_along(axis, |_| offset);
        self.set_offset(offset, cx);
        cx.stop_propagation();
    }

    fn handle_hover_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let hovered = self.parent_hovered(window);
        let crossed = hovered != self.was_hovered;
        self.was_hovered = hovered;

        if hovered || crossed {
            self.activity.mark();
        }

        if hovered {
            self.update_hover(&event.position, cx);
        } else {
            self.set_thumb(ThumbState::Inactive, cx);
        }
        if crossed {
            cx.notify();
        }
    }
}

impl Render for ScrollbarState {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        ScrollbarElement { state: cx.entity() }
    }
}

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

enum MouseTarget {
    TrackClick,
    ThumbDrag(Pixels),
}

struct ScrollbarLayout {
    axis: ScrollbarAxis,
    track_bounds: Bounds<Pixels>,
    thumb_bounds: Bounds<Pixels>,
    fill_bounds: Bounds<Pixels>,
    hitbox: Hitbox,
}

impl ScrollbarLayout {
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

impl ScrollbarElement {
    fn register_handlers(&self, capture_phase: DispatchPhase, window: &mut Window) {
        self.on_mouse_down(capture_phase, window);
        self.on_mouse_move(capture_phase, window);
        self.on_mouse_up(capture_phase, window);
        self.on_scroll_wheel(window);
    }

    fn on_mouse_down(&self, capture_phase: DispatchPhase, window: &mut Window) {
        let state = self.state.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, _window, cx| {
            if phase != capture_phase || event.button != MouseButton::Left {
                return;
            }
            state.update(cx, |state, cx| state.handle_mouse_down(event, cx));
        });
    }

    fn on_mouse_move(&self, capture_phase: DispatchPhase, window: &mut Window) {
        let state = self.state.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if phase != capture_phase {
                return;
            }

            let dragging = match state.read(cx).thumb {
                ThumbState::Dragging(axis, grab) if event.dragging() => Some((axis, grab)),
                _ => None,
            };

            state.update(cx, |state, cx| match dragging {
                Some((axis, grab)) => state.handle_drag(event, axis, grab, cx),
                None if event.pressed_button.is_none() => {
                    state.handle_hover_move(event, window, cx)
                }
                None => {}
            });
        });
    }

    fn on_mouse_up(&self, capture_phase: DispatchPhase, window: &mut Window) {
        let state = self.state.clone();
        window.on_mouse_event(move |event: &MouseUpEvent, phase, _window, cx| {
            if phase != capture_phase {
                return;
            }
            state.update(cx, |state, cx| state.update_hover(&event.position, cx));
        });
    }

    fn on_scroll_wheel(&self, window: &mut Window) {
        let state = self.state.clone();
        window.on_mouse_event(move |_: &ScrollWheelEvent, phase, window, cx| {
            if phase.bubble() {
                state.update(cx, |state, cx| {
                    if state.parent_hovered(window) {
                        state.mark_scrolled(cx);
                    }
                });
            }
        });
    }
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
        _inspector_id: Option<&InspectorElementId>,
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
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let state = self.state.read(cx);
        let axes = state.scrollable_axes().collect::<SmallVec<[_; 2]>>();

        let handle = &state.handle;
        let max_offset = handle.max_offset();
        let offset = handle.offset();

        let viewport_bounds = handle.bounds();
        let viewport = viewport_bounds.size;

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
        let bounds = layout.parent_hitbox.bounds;

        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            let (dragging, opacity) = {
                let state = self.state.read(cx);
                (state.thumb.is_dragging(), state.opacity(hovered))
            };

            if opacity > 0. {
                let theme = cx.theme();
                let state = self.state.read(cx);

                if dragging {
                    window.set_window_cursor_style(CursorStyle::Arrow);
                }

                for bar in &layout.bars {
                    let active = matches!(
                        state.thumb,
                        ThumbState::Dragging(axis, _) | ThumbState::Hover(axis) if axis == bar.axis
                    );

                    let color = if active {
                        theme.colors.secondary
                    } else {
                        theme.colors.tertiary
                    }
                    .opacity(opacity);

                    window.paint_quad(quad(
                        bar.fill_bounds,
                        Corners::all(Pixels::MAX).clamp_radii_for_quad_size(bar.fill_bounds.size),
                        color,
                        Edges::default(),
                        Hsla::transparent_black(),
                        BorderStyle::default(),
                    ));

                    if !dragging {
                        window.set_cursor_style(CursorStyle::Arrow, &bar.hitbox);
                    }
                }

                let scroll_opacity = state.scroll_opacity();
                let fading =
                    (opacity > 0. && opacity < 1.) || (scroll_opacity > 0. && scroll_opacity < 1.);

                if fading {
                    window.request_animation_frame();
                } else if state.needs_fade_wakeup(hovered) {
                    self.state.update(cx, |state, cx| state.schedule_fade(cx));
                }
            }

            let capture_phase = if dragging {
                DispatchPhase::Capture
            } else {
                DispatchPhase::Bubble
            };

            self.state
                .update(cx, |state, _| state.last_layout = Some(layout));

            self.register_handlers(capture_phase, window);
        });
    }
}

impl IntoElement for ScrollbarElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
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

    /// Dragging must be idempotent. Holding the thumb should land on the
    /// same offset every frame, and the thumb should never overflow its track.
    #[track_caller]
    fn check_round_trips(track: Pixels, viewport: Pixels, max_offset: Pixels) {
        for step in 0..=100 {
            let offset = -max_offset * (step as f32 / 100.);
            let (start, size) =
                thumb_placement(track, viewport, max_offset, offset).expect("thumb should exist");

            assert!(
                start + size <= track + px(0.5),
                "thumb {start:?}+{size:?} overflows track {track:?}",
            );

            let recovered = offset_for_thumb_start(track, size, start, max_offset);
            assert!(
                (recovered - offset).abs() < px(0.5),
                "offset {offset:?} placed thumb at {start:?} but mapped back to {recovered:?}",
            );
        }
    }

    #[test]
    fn thumb_placement_round_trips() {
        check_round_trips(VIEWPORT, VIEWPORT, MAX_OFFSET);
    }

    #[test]
    fn thumb_placement_round_trips_when_track_differs_from_viewport() {
        // Both bars are visible, so the vertical track is shortened by the corner.
        check_round_trips(VIEWPORT - SCROLLBAR_WIDTH, VIEWPORT, MAX_OFFSET);
    }

    #[track_caller]
    fn check_hidden(track: Pixels, viewport: Pixels, max_offset: Pixels) {
        assert!(thumb_placement(track, viewport, max_offset, Pixels::ZERO).is_none());
    }

    #[test]
    fn thumb_is_hidden_when_there_is_nothing_to_scroll() {
        check_hidden(px(900.), px(900.), Pixels::ZERO);
        check_hidden(Pixels::ZERO, px(900.), px(100.));
        // Track is too short to fit the thumb.
        check_hidden(px(10.), px(900.), px(100.));
    }

    #[test]
    fn thumb_never_shrinks_below_the_minimum() {
        let (_, size) = thumb_placement(px(900.), px(900.), px(1_000_000.), Pixels::ZERO).unwrap();
        assert_eq!(size, MIN_THUMB_SIZE);
    }
}
