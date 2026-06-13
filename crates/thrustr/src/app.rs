use crate::globals::PluginServiceExt;
use crate::navigation::{Navigator, NavigatorExt, Page};
use config::paths;
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyView, App as GpuiApp, AppContext, Context, FocusHandle, Focusable, FontWeight,
    InteractiveElement, IntoElement, ParentElement, Render, RenderOnce, SharedString,
    StatefulInteractiveElement, Styled, Window, div, rems, svg, transparent_black,
};
use theme::ThemeExt;
use ui::UiProvider;

#[derive(IntoElement)]
struct SidebarIconButton {
    page: Page,
}

impl From<Page> for SidebarIconButton {
    fn from(page: Page) -> Self {
        Self { page }
    }
}

impl RenderOnce for SidebarIconButton {
    fn render(self, _window: &mut Window, cx: &mut GpuiApp) -> impl IntoElement {
        let theme = cx.theme();

        let label = self.page.label();
        let icon_path = self.page.icon_path();
        let is_active = cx.navigator().is_active_for(self.page.clone());

        div()
            .id(label)
            .cursor_pointer()
            .on_click(move |_, _, cx| cx.navigate(self.page.clone()))
            .group(label)
            .p(rems(0.625))
            .flex()
            .items_center()
            .justify_center()
            .rounded(theme.radius.full)
            .bg(transparent_black())
            .hover(|div| div.bg(theme.colors.sidebar_hover))
            .when(is_active, |div| div.bg(theme.colors.sidebar_surface))
            .child(
                svg()
                    .group(label)
                    .path(icon_path)
                    .text_color(theme.colors.sidebar_secondary)
                    .size(rems(1.5))
                    .when(is_active, |svg| {
                        svg.text_color(theme.colors.sidebar_primary)
                    }),
            )
    }
}

#[derive(IntoElement)]
pub struct Sidebar;

impl RenderOnce for Sidebar {
    fn render(self, _window: &mut Window, cx: &mut GpuiApp) -> impl IntoElement {
        let theme = cx.theme();

        let top_nav = div()
            .flex()
            .flex_col()
            .items_center()
            .gap(rems(0.75))
            .child(SidebarIconButton::from(Page::Home))
            .child(SidebarIconButton::from(Page::Library))
            .child(SidebarIconButton::from(Page::Collections));

        let bottom_nav = div()
            .flex()
            .flex_col()
            .items_center()
            .gap(rems(0.75))
            .mb(rems(1.5))
            .child(SidebarIconButton::from(Page::Settings(None)));

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
                div()
                    .flex()
                    .flex_grow_1()
                    .flex_col()
                    .items_center()
                    .justify_between()
                    .child(top_nav)
                    .child(bottom_nav),
            )
    }
}

#[derive(IntoElement)]
pub struct Topbar {
    title: SharedString,
}

impl Topbar {
    fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
        }
    }
}

impl RenderOnce for Topbar {
    fn render(self, _window: &mut Window, cx: &mut GpuiApp) -> impl IntoElement {
        let theme = cx.theme();

        div()
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
    }
}

pub struct App {
    current_page: Page,
    active_view: AnyView,
    focus_handle: FocusHandle,
}

impl App {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let current_page = cx.navigator().current_page();
        let active_view = current_page.build_view(cx);

        cx.observe_global_in::<Navigator>(window, |this, window, cx| {
            let page = cx.navigator().current_page();
            this.active_view = page.build_view(cx);
            this.current_page = page;
            this.focus_handle.focus(window, cx);
            cx.notify();
        })
        .detach();

        Self::load_plugins(cx);

        Self {
            current_page,
            active_view,
            focus_handle: cx.focus_handle(),
        }
    }

    fn load_plugins(cx: &mut Context<Self>) {
        let plugin_manager = cx.plugin_service();
        cx.background_spawn(async move {
            let _ = plugin_manager
                .load_and_init(paths::plugins_dir().as_path())
                .await;
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
                div().flex().size_full().child(Sidebar).child(
                    div()
                        .flex_grow_1()
                        .flex()
                        .flex_col()
                        .child(Topbar::new(self.current_page.label()))
                        .child(self.active_view.clone()),
                ),
            )
            .children(UiProvider::render_dialogs(window, cx))
    }
}
