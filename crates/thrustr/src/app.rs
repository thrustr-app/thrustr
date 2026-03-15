use crate::extensions::EventListenerExt;
use crate::globals::ComponentManagerExt;
use crate::navigation::{NavigationExt, Navigator, Page};
use crate::{
    components::{Sidebar, Topbar},
    globals::PluginManagerExt,
};
use config::paths;
use gpui::{AnyView, AppContext, Context, IntoElement, ParentElement, Render, Styled, Window, div};
use gpui_tokio::Tokio;
use theme_manager::ThemeExt;
use ui::UiProvider;

pub struct App {
    current_page: Page,
    active_view: AnyView,
}

impl App {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let current_page = cx.current_page();
        let active_view = current_page.build_view(cx);

        cx.observe_global::<Navigator>(|this, cx| {
            let page = cx.current_page();
            this.active_view = page.build_view(cx);
            this.current_page = page;
            cx.notify();
        })
        .detach();

        cx.listen("plugin", |app, cx| {
            app.init_components(cx);
        })
        .detach();

        let app = Self {
            current_page,
            active_view,
        };
        app.load_plugins(cx);
        app
    }

    fn load_plugins(&self, cx: &mut Context<Self>) {
        let plugin_manager = cx.plugin_manager();
        cx.background_spawn(async move {
            let _ = plugin_manager
                .load_plugins(paths::plugins_dir().as_path())
                .await;
        })
        .detach();
    }

    fn init_components(&self, cx: &mut Context<Self>) {
        let components = cx.components();
        for component in components {
            Tokio::spawn(cx, async move {
                let _ = component.init().await;
            })
            .detach();
        }
    }
}

impl Render for App {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        div()
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
