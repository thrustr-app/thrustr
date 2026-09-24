use super::{ROUTE_PADDING, RouteHandle};
use crate::navigation::{NavNode, NavSidebar, Navigator, NavigatorExt, Page, nav_item};
use gpui::{
    Animation, AnimationExt, AnyElement, App, Context, Corners, FocusHandle, FontWeight,
    InteractiveElement, IntoElement, ParentElement, Render, RenderOnce, SharedString, Styled,
    Window, div, px, relative, rems, svg,
};
use std::time::Duration;
use theme::ThemeExt;
use ui::{ALL_CORNERS, ClientDecorations, CloseWindow, Sidebar, TitleBar, client_side_decorations};

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

#[derive(IntoElement)]
struct Topbar {
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
        let active_view = current_page.build_view(cx.navigator().current_state(), window, cx);

        let root = cx.weak_entity();
        cx.global_mut::<Navigator>()
            .set_state_saver(move |next, cx| {
                root.upgrade()?.read(cx).active_view.save_state(next, cx)
            });

        cx.observe_global_in::<Navigator>(window, |this, window, cx| {
            let page = cx.navigator().current_page();

            if this.current_page.section() != page.section() {
                let state = cx.navigator().current_state();
                this.active_view = page.build_view(state, window, cx);
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
