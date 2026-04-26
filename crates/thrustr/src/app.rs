use crate::navigation::{NavigationExt, Navigator, Page};
use crate::{
    components::{Sidebar, Topbar},
    globals::PluginServiceExt,
};
use config::paths;
use gpui::{
    AnyView, App as GpuiApp, AppContext, Context, FocusHandle, Focusable, InteractiveElement,
    IntoElement, ParentElement, Render, Styled, Window, div,
};
use theme::ThemeExt;
use ui::UiProvider;

pub struct App {
    current_page: Page,
    active_view: AnyView,
    focus_handle: FocusHandle,
}

impl App {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let current_page = cx.current_page();
        let active_view = current_page.build_view(cx);

        cx.observe_global_in::<Navigator>(window, |this, window, cx| {
            let page = cx.current_page();
            this.active_view = page.build_view(cx);
            this.current_page = page;
            this.focus_handle.focus(window, cx);
            cx.notify();
        })
        .detach();

        let app = Self {
            current_page,
            active_view,
            focus_handle: cx.focus_handle(),
        };
        app.load_plugins(cx);
        app
    }

    fn load_plugins(&self, cx: &mut Context<Self>) {
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
            .track_focus(&self.focus_handle(cx))
            .size_full()
            .bg(theme.colors.background)
            .child(
                div().flex().size_full().child(Sidebar::new()).child(
                    div()
                        .flex_grow()
                        .flex()
                        .flex_col()
                        .child(Topbar::new(self.current_page.label()))
                        .child(self.active_view.clone()),
                ),
            )
            .children(UiProvider::render_dialogs(window, cx))
    }
}
