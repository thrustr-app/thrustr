use crate::navigation::{NavNode, NavSidebar, Navigator, NavigatorExt, Page, nav_item};
use gpui::{
    AnyElement, AnyView, App as GpuiApp, Context, Corners, EmptyView, Entity, FocusHandle,
    Focusable, FontWeight, InteractiveElement, IntoElement, ParentElement, Render, RenderOnce,
    SharedString, Styled, Window, div, relative, rems, svg,
};
use theme::ThemeExt;
use ui::{
    ALL_CORNERS, ClientDecorations, CloseWindow, Sidebar, TitleBar, UiProvider,
    client_side_decorations,
};

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

fn sidebar(window: &Window, cx: &GpuiApp) -> impl IntoElement {
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

pub trait Route: Render {
    fn header(&mut self, _cx: &mut Context<Self>) -> Option<AnyElement> {
        None
    }
}

pub trait RouteHandle {
    fn view(&self) -> AnyView;
    fn render_header(&self, cx: &mut GpuiApp) -> Option<AnyElement>;
}

impl Route for EmptyView {}

impl<T: Route> RouteHandle for Entity<T> {
    fn view(&self) -> AnyView {
        self.clone().into()
    }

    fn render_header(&self, cx: &mut GpuiApp) -> Option<AnyElement> {
        self.update(cx, |page, cx| page.header(cx))
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
    fn render(self, _window: &mut Window, cx: &mut GpuiApp) -> impl IntoElement {
        let theme = cx.theme();

        div()
            .px(rems(3.))
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

pub struct App {
    current_page: Page,
    active_view: Box<dyn RouteHandle>,
    focus_handle: FocusHandle,
}

impl App {
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

impl Focusable for App {
    fn focus_handle(&self, _: &GpuiApp) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for App {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        let root = div()
            .font_family("Sora")
            .track_focus(&self.focus_handle(cx))
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
                            .child(Topbar::new(
                                self.current_page.label(),
                                self.active_view.render_header(cx),
                            ))
                            .child(self.active_view.view()),
                    ),
            )
            .children(UiProvider::render_dialogs(window, cx));

        client_side_decorations(root, window, cx)
    }
}
