// SPDX-License-Identifier: GPL-3.0-or-later
//
// The window controls, title-bar dragging, and traffic-light metrics are adapted
// from Zed's `platform_title_bar` and `ui` crates, Copyright (C) Zed Industries,
// Inc., licensed under GPL-3.0-or-later:
// https://github.com/zed-industries/zed
// Modified and redistributed as part of Thrustr under GPL-3.0-or-later.

use crate::ClientDecorations;
use gpui::{
    AnyElement, App, Corners, Decorations, ElementId, FontWeight, Hsla, InteractiveElement,
    IntoElement, MouseButton, ParentElement, Pixels, Point, RenderOnce, Rgba, SharedString,
    StatefulInteractiveElement, Styled, Window, WindowButton, WindowButtonLayout,
    WindowControlArea, actions, div, point, prelude::FluentBuilder, px, rems, svg,
};
use theme::ThemeExt;

actions!(title_bar, [CloseWindow]);

pub const TITLE_BAR_HEIGHT: gpui::Rems = gpui::Rems(2.);

pub const TRAFFIC_LIGHT_POSITION: Point<Pixels> = point(px(13.), px(9.));

const TRAFFIC_LIGHT_WIDTH: f32 = 75.;

const CAPTION_BUTTON_WIDTH: f32 = 46.;

#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum PlatformStyle {
    Windows,
    Linux,
    Macos,
}

impl PlatformStyle {
    #[cfg(target_os = "macos")]
    const CURRENT: Self = Self::Macos;
    #[cfg(target_os = "windows")]
    const CURRENT: Self = Self::Windows;
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    const CURRENT: Self = Self::Linux;
}

/// A press on the title bar that has not turned into a window move yet.
#[derive(Default)]
struct TitleBarDrag {
    pressed: bool,
}

#[derive(IntoElement)]
pub struct TitleBar {
    id: ElementId,
    title: Option<SharedString>,
}

impl TitleBar {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            title: None,
        }
    }

    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }
}

impl RenderOnce for TitleBar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let platform = PlatformStyle::CURRENT;
        let macos = platform == PlatformStyle::Macos;
        let app_moves_window = platform != PlatformStyle::Windows;

        let client_decorated = matches!(window.window_decorations(), Decorations::Client { .. });
        let linux_csd = platform == PlatformStyle::Linux && client_decorated;

        let (left, right) = match platform {
            _ if window.is_fullscreen() => (None, None),
            PlatformStyle::Windows => (None, Some(windows_controls().into_any_element())),
            PlatformStyle::Linux if client_decorated => {
                let layout = cx.button_layout().unwrap_or(WindowButtonLayout {
                    left: [None; gpui::MAX_BUTTONS_PER_SIDE],
                    right: [
                        Some(WindowButton::Minimize),
                        Some(WindowButton::Maximize),
                        Some(WindowButton::Close),
                    ],
                });
                (
                    linux_controls("left-controls", layout.left, window),
                    linux_controls("right-controls", layout.right, window),
                )
            }
            _ => (None, None),
        };

        let leading_pad = macos && !window.is_fullscreen();
        let drag = window.use_keyed_state("title-bar-drag", cx, |_, _| TitleBarDrag::default());

        div()
            .id(self.id)
            .window_control_area(WindowControlArea::Drag)
            .relative()
            .flex()
            .items_center()
            .w_full()
            .h(TITLE_BAR_HEIGHT)
            .flex_shrink_0()
            .bg(theme.colors.background)
            .border_b_1()
            .border_color(theme.colors.border)
            .rounded_client_corners(
                Corners {
                    top_left: true,
                    top_right: true,
                    ..Default::default()
                },
                window,
            )
            .when(leading_pad, |this| this.pl(px(TRAFFIC_LIGHT_WIDTH)))
            .when(app_moves_window, |this| {
                this.on_mouse_down(MouseButton::Left, {
                    let drag = drag.clone();
                    move |_, _, cx| drag.update(cx, |drag, _| drag.pressed = true)
                })
                .on_mouse_up(MouseButton::Left, {
                    let drag = drag.clone();
                    move |_, _, cx| drag.update(cx, |drag, _| drag.pressed = false)
                })
                .on_mouse_down_out({
                    let drag = drag.clone();
                    move |_, _, cx| drag.update(cx, |drag, _| drag.pressed = false)
                })
                .on_mouse_move(move |_, window, cx| {
                    if drag.read(cx).pressed {
                        drag.update(cx, |drag, _| drag.pressed = false);
                        window.start_window_move();
                    }
                })
                .on_click(move |event, window, _| {
                    if event.click_count() == 2 {
                        if macos {
                            window.titlebar_double_click();
                        } else {
                            window.zoom_window();
                        }
                    }
                })
            })
            .when(linux_csd && window.window_controls().window_menu, |this| {
                this.on_mouse_down(MouseButton::Right, |event, window, _| {
                    window.show_window_menu(event.position)
                })
            })
            .children(self.title.map(|title| {
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .mt_0p5()
                            .text_size(rems(0.875))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.colors.secondary)
                            .child(title),
                    )
            }))
            .children(left)
            .child(div().flex_grow_1())
            .children(right)
    }
}

fn windows_controls() -> impl IntoElement {
    div()
        .font_family("Segoe Fluent Icons")
        .flex()
        .h_full()
        .child(CaptionButton::Minimize)
        .child(CaptionButton::Maximize)
        .child(CaptionButton::Close)
}

#[derive(IntoElement, Clone, Copy)]
enum CaptionButton {
    Minimize,
    Maximize,
    Close,
}

impl CaptionButton {
    fn id(self) -> &'static str {
        match self {
            Self::Minimize => "minimize",
            Self::Maximize => "maximize",
            Self::Close => "close",
        }
    }

    fn control_area(self) -> WindowControlArea {
        match self {
            Self::Minimize => WindowControlArea::Min,
            Self::Maximize => WindowControlArea::Max,
            Self::Close => WindowControlArea::Close,
        }
    }
}

impl RenderOnce for CaptionButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();

        let glyph = match self {
            Self::Minimize => "\u{e921}",
            Self::Maximize if window.is_maximized() => "\u{e923}",
            Self::Maximize => "\u{e922}",
            Self::Close => "\u{e8bb}",
        };

        let close_bg: Hsla = Rgba {
            r: 196. / 255.,
            g: 43. / 255.,
            b: 28. / 255.,
            a: 1.,
        }
        .into();
        let (hover_bg, hover_fg) = match self {
            Self::Close => (close_bg, gpui::white()),
            _ => (theme.colors.hover, theme.colors.primary),
        };

        div()
            .id(self.id())
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .w(px(CAPTION_BUTTON_WIDTH))
            .h_full()
            .text_size(px(10.))
            .text_color(theme.colors.primary)
            .hover(|style| style.bg(hover_bg).text_color(hover_fg))
            .active(|style| {
                style
                    .bg(hover_bg.opacity(0.8))
                    .text_color(hover_fg.opacity(0.8))
            })
            .window_control_area(self.control_area())
            .child(glyph)
    }
}

fn linux_controls(
    id: &'static str,
    buttons: [Option<WindowButton>; gpui::MAX_BUTTONS_PER_SIDE],
    window: &Window,
) -> Option<AnyElement> {
    let supported = window.window_controls();
    let is_maximized = window.is_maximized();

    let controls: Vec<_> = buttons
        .into_iter()
        .flatten()
        .filter(|button| match button {
            WindowButton::Minimize => supported.minimize,
            WindowButton::Maximize => supported.maximize,
            WindowButton::Close => true,
        })
        .map(|button| WindowControl::new(button, is_maximized))
        .collect();

    if controls.is_empty() {
        return None;
    }

    Some(
        div()
            .id(id)
            .flex()
            .items_center()
            .gap(rems(0.5))
            .px(rems(0.5))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .children(controls)
            .into_any_element(),
    )
}

#[derive(IntoElement)]
struct WindowControl {
    button: WindowButton,
    is_maximized: bool,
}

impl WindowControl {
    fn new(button: WindowButton, is_maximized: bool) -> Self {
        Self {
            button,
            is_maximized,
        }
    }

    fn icon(&self) -> &'static str {
        match self.button {
            WindowButton::Minimize => "icons/minimize.svg",
            WindowButton::Maximize if self.is_maximized => "icons/restore.svg",
            WindowButton::Maximize => "icons/maximize.svg",
            WindowButton::Close => "icons/x.svg",
        }
    }
}

impl RenderOnce for WindowControl {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let button = self.button;

        div()
            .id(button.id())
            .group("window-control")
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .size(rems(1.25))
            .rounded_full()
            .cursor_pointer()
            .hover(|style| style.bg(theme.colors.hover))
            .child(
                svg()
                    .size(rems(0.875))
                    .path(self.icon())
                    .text_color(theme.colors.secondary)
                    .group_hover("window-control", |this| {
                        this.text_color(theme.colors.primary)
                    }),
            )
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                match button {
                    WindowButton::Minimize => window.minimize_window(),
                    WindowButton::Maximize => window.zoom_window(),
                    WindowButton::Close => window.dispatch_action(Box::new(CloseWindow), cx),
                }
            })
    }
}
