// SPDX-License-Identifier: GPL-3.0-or-later
//
// Adapted from Zed's `workspace` and `theme` crates, Copyright (C) Zed
// Industries, Inc., licensed under GPL-3.0-or-later:
// https://github.com/zed-industries/zed
// Modified and redistributed as part of Thrustr under GPL-3.0-or-later.

use gpui::{
    App, Bounds, BoxShadow, Corners, CursorStyle, Decorations, Div, Global, Hitbox, HitboxBehavior,
    InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels, Point, ResizeEdge, Size,
    Stateful, Styled, Tiling, Window, canvas, div, hsla, point, prelude::FluentBuilder, px, size,
    transparent_black,
};
use theme::ThemeExt;

pub const CLIENT_SIDE_DECORATION_SHADOW: Pixels = px(10.);
pub const CLIENT_SIDE_DECORATION_ROUNDING: Pixels = px(10.);

pub const ALL_CORNERS: Corners<bool> = Corners {
    top_left: true,
    top_right: true,
    bottom_right: true,
    bottom_left: true,
};

/// Rounds the corners of elements that paint over a window corner.
pub trait ClientDecorations: Styled + Sized {
    fn rounded_client_corners(self, corners: Corners<bool>, window: &Window) -> Self {
        match window.window_decorations() {
            Decorations::Server => self,
            Decorations::Client { tiling } => rounded_corners(self, corners, tiling),
        }
    }
}

impl<T: Styled> ClientDecorations for T {}

struct GlobalResizeEdge(ResizeEdge);
impl Global for GlobalResizeEdge {}

pub fn client_side_decorations(
    element: impl IntoElement,
    window: &mut Window,
    cx: &mut App,
) -> Stateful<Div> {
    const BORDER_SIZE: Pixels = px(1.);
    let decorations = window.window_decorations();
    let tiling = match decorations {
        Decorations::Server => Tiling::default(),
        Decorations::Client { tiling } => tiling,
    };

    match decorations {
        Decorations::Client { .. } => window.set_client_inset(CLIENT_SIDE_DECORATION_SHADOW),
        Decorations::Server => window.set_client_inset(px(0.)),
    }

    let border = cx.theme().colors.border;

    div()
        .id("window-backdrop")
        .bg(transparent_black())
        .map(|div| match decorations {
            Decorations::Server => div,
            Decorations::Client { .. } => rounded_corners(div, ALL_CORNERS, tiling)
                .when(!tiling.top, |div| div.pt(CLIENT_SIDE_DECORATION_SHADOW))
                .when(!tiling.bottom, |div| div.pb(CLIENT_SIDE_DECORATION_SHADOW))
                .when(!tiling.left, |div| div.pl(CLIENT_SIDE_DECORATION_SHADOW))
                .when(!tiling.right, |div| div.pr(CLIENT_SIDE_DECORATION_SHADOW))
                .on_mouse_move(move |e, window, cx| {
                    let size = window.window_bounds().get_bounds().size;
                    let new_edge =
                        resize_edge(e.position, CLIENT_SIDE_DECORATION_SHADOW, size, tiling);
                    let edge = cx.try_global::<GlobalResizeEdge>().map(|e| e.0);
                    if new_edge != edge {
                        window
                            .window_handle()
                            .update(cx, |view, _, cx| cx.notify(view.entity_id()))
                            .ok();
                    }
                })
                .on_mouse_down(MouseButton::Left, move |e, window, _| {
                    let size = window.window_bounds().get_bounds().size;
                    if let Some(edge) =
                        resize_edge(e.position, CLIENT_SIDE_DECORATION_SHADOW, size, tiling)
                    {
                        window.start_window_resize(edge);
                    }
                }),
        })
        .size_full()
        .child(
            div()
                .cursor(CursorStyle::Arrow)
                .map(|div| match decorations {
                    Decorations::Server => div,
                    Decorations::Client { .. } => rounded_corners(div, ALL_CORNERS, tiling)
                        .border_color(border)
                        .when(!tiling.top, |div| div.border_t(BORDER_SIZE))
                        .when(!tiling.bottom, |div| div.border_b(BORDER_SIZE))
                        .when(!tiling.left, |div| div.border_l(BORDER_SIZE))
                        .when(!tiling.right, |div| div.border_r(BORDER_SIZE))
                        .when(!tiling.is_tiled(), |div| {
                            div.shadow(vec![
                                BoxShadow::new(px(0.), px(0.), hsla(0., 0., 0., 0.4))
                                    .blur_radius(CLIENT_SIDE_DECORATION_SHADOW / 2.),
                            ])
                        }),
                })
                .on_mouse_move(|_, _, cx| cx.stop_propagation())
                .size_full()
                .child(element),
        )
        .map(|div| match decorations {
            Decorations::Server => div,
            Decorations::Client { tiling } => div.child(
                canvas(
                    |_, window, _| {
                        window.insert_hitbox(
                            Bounds::new(
                                point(px(0.), px(0.)),
                                window.window_bounds().get_bounds().size,
                            ),
                            HitboxBehavior::Normal,
                        )
                    },
                    move |_, hitbox: Hitbox, window, cx| {
                        let mouse = window.mouse_position();
                        let size = window.window_bounds().get_bounds().size;
                        let Some(edge) =
                            resize_edge(mouse, CLIENT_SIDE_DECORATION_SHADOW, size, tiling)
                        else {
                            return;
                        };
                        cx.set_global(GlobalResizeEdge(edge));
                        window.set_cursor_style(resize_cursor(edge), &hitbox);
                    },
                )
                .size_full()
                .absolute(),
            ),
        })
}

fn resize_cursor(edge: ResizeEdge) -> CursorStyle {
    match edge {
        ResizeEdge::Top | ResizeEdge::Bottom => CursorStyle::ResizeUpDown,
        ResizeEdge::Left | ResizeEdge::Right => CursorStyle::ResizeLeftRight,
        ResizeEdge::TopLeft | ResizeEdge::BottomRight => CursorStyle::ResizeUpLeftDownRight,
        ResizeEdge::TopRight | ResizeEdge::BottomLeft => CursorStyle::ResizeUpRightDownLeft,
    }
}

fn rounded_corners<E: Styled>(mut el: E, corners: Corners<bool>, tiling: Tiling) -> E {
    if corners.top_left && !tiling.top && !tiling.left {
        el = el.rounded_tl(CLIENT_SIDE_DECORATION_ROUNDING);
    }
    if corners.top_right && !tiling.top && !tiling.right {
        el = el.rounded_tr(CLIENT_SIDE_DECORATION_ROUNDING);
    }
    if corners.bottom_left && !tiling.bottom && !tiling.left {
        el = el.rounded_bl(CLIENT_SIDE_DECORATION_ROUNDING);
    }
    if corners.bottom_right && !tiling.bottom && !tiling.right {
        el = el.rounded_br(CLIENT_SIDE_DECORATION_ROUNDING);
    }
    el
}

fn resize_edge(
    pos: Point<Pixels>,
    shadow: Pixels,
    window_size: Size<Pixels>,
    tiling: Tiling,
) -> Option<ResizeEdge> {
    let bounds = Bounds::new(Point::default(), window_size).inset(shadow * 1.5);
    if bounds.contains(&pos) {
        return None;
    }

    let corner = size(shadow * 1.5, shadow * 1.5);
    let top_left = Bounds::new(point(px(0.), px(0.)), corner);
    if !tiling.top && top_left.contains(&pos) {
        return Some(ResizeEdge::TopLeft);
    }
    let top_right = Bounds::new(point(window_size.width - corner.width, px(0.)), corner);
    if !tiling.top && top_right.contains(&pos) {
        return Some(ResizeEdge::TopRight);
    }
    let bottom_left = Bounds::new(point(px(0.), window_size.height - corner.height), corner);
    if !tiling.bottom && bottom_left.contains(&pos) {
        return Some(ResizeEdge::BottomLeft);
    }
    let bottom_right = Bounds::new(
        point(
            window_size.width - corner.width,
            window_size.height - corner.height,
        ),
        corner,
    );
    if !tiling.bottom && bottom_right.contains(&pos) {
        return Some(ResizeEdge::BottomRight);
    }

    if !tiling.top && pos.y < shadow {
        Some(ResizeEdge::Top)
    } else if !tiling.bottom && pos.y > window_size.height - shadow {
        Some(ResizeEdge::Bottom)
    } else if !tiling.left && pos.x < shadow {
        Some(ResizeEdge::Left)
    } else if !tiling.right && pos.x > window_size.width - shadow {
        Some(ResizeEdge::Right)
    } else {
        None
    }
}
