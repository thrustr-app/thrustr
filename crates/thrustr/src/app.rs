use crate::globals::PluginServiceExt;
use crate::navigation::{NavNode, Navigator, NavigatorExt, Page, nav_item};
use config::paths;
use gpui::{
    AnyElement, AnyView, App as GpuiApp, AppContext, Context, EmptyView, Entity, FocusHandle,
    Focusable, FontWeight, InteractiveElement, IntoElement, ParentElement, Render, RenderOnce,
    SharedString, Styled, Window, div, rems, svg,
};
use theme::ThemeExt;
use tracing::error;
use ui::{Sidebar, UiProvider};

fn sidebar_rail(cx: &GpuiApp) -> impl IntoElement {
    let theme = cx.theme();

    div()
        .flex()
        .flex_col()
        .gap(rems(2.))
        .items_center()
        .flex_shrink_0()
        .w(rems(5.5))
        .bg(theme.colors.sidebar_background)
        .border_r_1()
        .border_color(theme.colors.border)
        .child(
            svg()
                .path("icons/logo.svg")
                .text_color(theme.colors.sidebar_logo)
                .mt(rems(1.5))
                .size(rems(3.)),
        )
        .child(
            Sidebar::new("main-sidebar")
                .flex_grow_1()
                .pb(rems(1.5))
                .item(nav_item(Page::Home, cx))
                .item(nav_item(Page::Library, cx))
                .item(nav_item(Page::Collections, cx))
                .bottom_item(nav_item(Page::Settings(None), cx)),
        )
}

/// A top-level page.
pub trait Route: Render {
    /// Page-specific top bar content.
    fn header(&mut self, _cx: &mut Context<Self>) -> Option<AnyElement> {
        None
    }
}

/// Type-erased handle to the active [`Route`].
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
            .relative()
            .px(rems(2.))
            .h(rems(6.))
            .bg(theme.colors.background)
            .w_full()
            .flex_shrink_0()
            .flex()
            .items_center()
            .child(
                div()
                    .child(self.title)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(rems(1.5))
                    .text_color(theme.colors.primary),
            )
            .children(self.header.map(|header| {
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(header)
            }))
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
        let active_view = current_page.build_view(cx);

        cx.observe_global_in::<Navigator>(window, |this, _, cx| {
            let page = cx.navigator().current_page();

            if this.current_page.section() != page.section() {
                this.active_view = page.build_view(cx);
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

        Self::load_plugins(cx);

        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);

        Self {
            current_page,
            active_view,
            focus_handle,
        }
    }

    fn load_plugins(cx: &mut Context<Self>) {
        let plugin_manager = cx.plugin_service();
        cx.background_spawn(async move {
            if let Err(err) = plugin_manager
                .load_and_init(paths::plugins_dir().as_path())
                .await
            {
                error!("failed to load plugins: {err:#}");
            }
        })
        .detach();
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

        div()
            .font_family("Metropolis")
            .track_focus(&self.focus_handle(cx))
            .size_full()
            .bg(theme.colors.background)
            .child(
                div().flex().size_full().child(sidebar_rail(cx)).child(
                    div()
                        .flex_grow_1()
                        .flex()
                        .flex_col()
                        .child(Topbar::new(
                            self.current_page.label(),
                            self.active_view.render_header(cx),
                        ))
                        .child(self.active_view.view()),
                ),
            )
            .children(UiProvider::render_dialogs(window, cx))
    }
}
