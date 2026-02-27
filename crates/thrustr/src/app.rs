use crate::routes::{
    Appearance, Collections, Home, Library, MainLayout, Plugins, SettingsLayout, Storefronts,
};
use gpui::{AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Window, div};
use gpui_router::{Route, Routes};
use theme_manager::ThemeExt;

pub struct App {
    home: Entity<Home>,
    library: Entity<Library>,
    collections: Entity<Collections>,
    settings_storefonts: Entity<Storefronts>,
    settings_plugins: Entity<Plugins>,
    settings_appearance: Entity<Appearance>,
}

impl App {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let library = cx.new(|_cx| Library);
        let home = cx.new(|_cx| Home);
        let collections = cx.new(|_cx| Collections);

        let settings_storefonts = cx.new(|cx| Storefronts::new(cx));
        let settings_plugins = cx.new(|cx| Plugins::new(cx));
        let settings_appearance = cx.new(|_cx| Appearance);

        Self {
            home,
            library,
            collections,
            settings_storefonts,
            settings_plugins,
            settings_appearance,
        }
    }
}

impl Render for App {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        let home = self.home.clone();
        let library = self.library.clone();
        let collections = self.collections.clone();

        let storefronts = self.settings_storefonts.clone();
        let plugins = self.settings_plugins.clone();
        let appearance = self.settings_appearance.clone();

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
                                    .element(move |_, _| storefronts.clone()),
                            )
                            .child(
                                Route::new()
                                    .path("plugins")
                                    .element(move |_, _| plugins.clone()),
                            )
                            .child(
                                Route::new()
                                    .path("appearance")
                                    .element(move |_, _| appearance.clone()),
                            ),
                    ),
            ),
        )
    }
}
