use crate::navigation::{NavNode, NavSidebar, Navigator, NavigatorExt, Page, nav_item};
use config::paths;
use gpui::{
    Animation, AnimationExt, AnyElement, AnyView, App, Context, Corners, EmptyView, Entity,
    FocusHandle, FontWeight, InteractiveElement, IntoElement, ParentElement, Rems, Render,
    RenderOnce, SharedString, Styled, Window, div, px, relative, rems, svg,
};
use std::{path::Path, sync::Arc, time::Duration};
use theme::ThemeExt;
use ui::{ALL_CORNERS, ClientDecorations, CloseWindow, Sidebar, TitleBar, client_side_decorations};

mod collections;
mod game;
mod home;
mod library;
mod settings;

pub use collections::*;
pub use game::*;
pub use home::*;
pub use library::*;
pub use settings::*;

fn sidebar(window: &Window, cx: &App) -> impl IntoElement {
    let theme = cx.theme();

    div()
        .flex()
        .flex_col()
        .gap(rems(1.75))
        .items_center()
        .flex_shrink_0()
        .w(rems(4.75))
        .bg(theme.colors.sidebar.background)
        .rounded_client_corners(
            Corners {
                bottom_left: true,
                ..Default::default()
            },
            window,
        )
        .border_r_1()
        .border_color(theme.colors.sidebar.border)
        .child(
            svg()
                .path("icons/logo.svg")
                .text_color(theme.colors.sidebar.logo)
                .mt(rems(1.5))
                .size(rems(3.)),
        )
        .child(
            Sidebar::main()
                .nav(cx.navigator().current_page())
                .flex_grow_1()
                .mb(rems(1.25))
                .item(nav_item(Page::Home))
                .item(nav_item(Page::Library))
                .item(nav_item(Page::Collections))
                .bottom_item(nav_item(Page::Settings(None))),
        )
}

pub const ROUTE_PADDING: Rems = rems(3.);

pub(crate) fn cover_path(hash: &str) -> Option<Arc<Path>> {
    paths::artwork_path(hash, "webp").ok().map(Into::into)
}

pub trait Route: Render {
    const PADDING: Rems = ROUTE_PADDING;
    const TOPBAR: bool = true;

    fn header(&self, _this: &Entity<Self>, _cx: &App) -> Option<AnyElement> {
        None
    }
}

pub trait RouteHandle {
    fn view(&self) -> AnyView;
    fn padding(&self) -> Rems;
    fn has_topbar(&self) -> bool;
    fn render_header(&self, cx: &App) -> Option<AnyElement>;
}

impl Route for EmptyView {}

impl<T: Route> RouteHandle for Entity<T> {
    fn view(&self) -> AnyView {
        self.clone().into()
    }

    fn padding(&self) -> Rems {
        T::PADDING
    }

    fn has_topbar(&self) -> bool {
        T::TOPBAR
    }

    fn render_header(&self, cx: &App) -> Option<AnyElement> {
        self.read(cx).header(self, cx)
    }
}

#[derive(IntoElement)]
pub struct Topbar {
    title: SharedString,
    header: Option<AnyElement>,
}

impl Topbar {
    fn new(title: impl Into<SharedString>, header: Option<AnyElement>) -> Self {
        Self {
            title: title.into(),
            header,
        }
    }
}

impl RenderOnce for Topbar {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();

        div()
            .px(ROUTE_PADDING)
            .h(rems(6.))
            .bg(theme.colors.background)
            .w_full()
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .child(self.title)
                    .font_weight(FontWeight::BOLD)
                    .text_size(theme.text.xl)
                    .line_height(relative(1.))
                    .text_color(theme.colors.primary),
            )
            .children(self.header)
    }
}

pub struct Root {
    current_page: Page,
    active_view: Box<dyn RouteHandle>,
    focus_handle: FocusHandle,
}

impl Root {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let current_page = cx.navigator().current_page();
        let active_view = current_page.build_view(window, cx);

        cx.observe_global_in::<Navigator>(window, |this, window, cx| {
            let page = cx.navigator().current_page();

            if this.current_page.section() != page.section() {
                this.active_view = page.build_view(window, cx);
            }
            this.current_page = page;

            cx.notify();
        })
        .detach();

        // When the focused element disappears, fall back to the
        // root handle so keyboard navigation keeps working.
        cx.on_focus_lost(window, |this, window, cx| {
            this.focus_handle.focus(window, cx);
        })
        .detach();

        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);

        Self {
            current_page,
            active_view,
            focus_handle,
        }
    }
}

impl Render for Root {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let active_view = self.active_view.view();

        let root = div()
            .track_focus(&self.focus_handle)
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.colors.background)
            .rounded_client_corners(ALL_CORNERS, window)
            .on_action(|_: &CloseWindow, window, _| window.remove_window())
            .child(TitleBar::new("title-bar").title("Thrustr"))
            .child(
                div()
                    .flex()
                    .flex_grow_1()
                    .min_h_0()
                    .child(sidebar(window, cx))
                    .child(
                        div()
                            .flex_grow_1()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .relative()
                            .with_animation(
                                ("page-transition", active_view.entity_id()),
                                Animation::new(Duration::from_millis(150)),
                                |page, delta| page.opacity(delta).top(px((1.0 - delta) * 6.0)),
                            )
                            .children(self.active_view.has_topbar().then(|| {
                                Topbar::new(
                                    self.current_page.label(),
                                    self.active_view.render_header(cx),
                                )
                            }))
                            .child(
                                div()
                                    .flex_grow_1()
                                    .flex()
                                    .flex_col()
                                    .min_h_0()
                                    .px(self.active_view.padding())
                                    .child(active_view),
                            ),
                    ),
            );

        client_side_decorations(root, window, cx)
    }
}
