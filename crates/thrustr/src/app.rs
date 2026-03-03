use crate::globals::{ComponentManagerExt, EventListenerExt};
use crate::navigation::Navigator;
use crate::{
    components::{Sidebar, Topbar},
    globals::PluginManagerExt,
    navigation::{self},
};
use config::paths;
use gpui::{AnyView, AppContext, Context, IntoElement, ParentElement, Render, Styled, Window, div};
use gpui_tokio::Tokio;
use theme_manager::ThemeExt;

pub struct App {
    active_view: AnyView,
}

impl App {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let active_view = navigation::current_page(cx).build_view(cx);

        cx.observe_global::<Navigator>(|this, cx| {
            let page = navigation::current_page(cx);
            this.active_view = page.build_view(cx);
            cx.notify();
        })
        .detach();

        cx.listen("plugin", |app, cx| {
            app.init_storefront(cx);
        })
        .detach();

        let app = Self { active_view };
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

    fn init_storefront(&self, cx: &mut Context<Self>) {
        let storefronts = cx.storefronts().into_iter();
        for storefront in storefronts {
            Tokio::spawn(cx, async move {
                let _ = storefront.component().init().await;
            })
            .detach();
        }
    }
}

impl Render for App {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        div().size_full().bg(theme.colors.background).child(
            div().flex().size_full().child(Sidebar::new()).child(
                div()
                    .flex_grow()
                    .flex()
                    .flex_col()
                    .child(Topbar::new())
                    .child(self.active_view.clone()),
            ),
        )
    }
}
