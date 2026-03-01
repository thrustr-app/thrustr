use crate::{
    globals::{PluginManagerExt, StorefrontManagerExt},
    routes::*,
};
use config::paths;
use gpui::{AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Window, div};
use gpui_router::{Route, Routes};
use gpui_tokio::Tokio;
use ports::managers::{PluginManager, StorefrontManager};
use theme_manager::ThemeExt;

pub struct App {
    home: Entity<Home>,
    library: Entity<Library>,
    collections: Entity<Collections>,
    settings_storefronts: Entity<Storefronts>,
    settings_plugins: Entity<Plugins>,
    settings_appearance: Entity<Appearance>,
}

impl App {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let library = cx.new(|_cx| Library);
        let home = cx.new(|_cx| Home);
        let collections = cx.new(|_cx| Collections);

        let settings_storefronts = cx.new(|cx| Storefronts::new(cx));
        let settings_plugins = cx.new(|cx| Plugins::new(cx));
        let settings_appearance = cx.new(|_cx| Appearance);

        let page = Self {
            home,
            library,
            collections,
            settings_storefronts,
            settings_plugins,
            settings_appearance,
        };

        cx.spawn(async |page, cx| {
            let mut listener = event::listen("plugin");
            while let Ok(_) = listener.recv().await {
                let _ = page.update(cx, |page, cx| {
                    page.init_storefront_providers(cx);
                });
            }
        })
        .detach();

        page.load_plugins(cx);
        page
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

    fn init_storefront_providers(&self, cx: &mut Context<Self>) {
        let inactive_providers = cx
            .storefront_manager()
            .storefront_providers()
            .into_iter()
            .filter(|p| p.status().is_inactive());

        for provider in inactive_providers {
            Tokio::spawn(cx, async move {
                let _ = provider.init().await;
            })
            .detach();
        }
    }
}

impl Render for App {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        let home = self.home.clone();
        let library = self.library.clone();
        let collections = self.collections.clone();

        let settings_storefronts = self.settings_storefronts.clone();
        let settings_plugins = self.settings_plugins.clone();
        let settings_appearance = self.settings_appearance.clone();

        div().size_full().bg(theme.colors.background).child(
            Routes::new().child(
                Route::new()
                    .layout(MainLayout::new())
                    .child(Route::new().index().element(move |_, _| home.clone()))
                    .child(
                        Route::new()
                            .path("collections")
                            .element(move |_, _| collections.clone()),
                    )
                    .child(
                        Route::new()
                            .path("library")
                            .element(move |_, _| library.clone()),
                    )
                    .child(
                        Route::new()
                            .path("settings")
                            .layout(SettingsLayout::new())
                            .child(
                                Route::new()
                                    .path("storefronts")
                                    .element(move |_, _| settings_storefronts.clone()),
                            )
                            .child(
                                Route::new()
                                    .path("plugins")
                                    .element(move |_, _| settings_plugins.clone()),
                            )
                            .child(
                                Route::new()
                                    .path("appearance")
                                    .element(move |_, _| settings_appearance.clone()),
                            ),
                    ),
            ),
        )
    }
}
