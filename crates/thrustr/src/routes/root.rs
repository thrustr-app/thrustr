use crate::{
    components::{Sidebar, Topbar},
    routes::{home::Home, library::Library, settings::Settings},
};
use gpui::{AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Window, div};
use gpui_router::{Route, Routes};
use theme_manager::ThemeExt;

mod home;
mod library;
mod settings;

pub struct Root {
    home: Entity<Home>,
    library: Entity<Library>,
    settings: Entity<Settings>,
}

impl Root {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let library = cx.new(|_cx| Library);
        let settings = cx.new(|_cx| Settings);
        let home = cx.new(|_cx| Home);

        Self {
            home,
            library,
            settings,
        }
    }
}

impl Render for Root {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        let home = self.home.clone();
        let library = self.library.clone();
        let settings = self.settings.clone();

        div()
            .absolute()
            .size_full()
            .bg(theme.colors.background)
            .flex()
            .child(Sidebar::new())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_grow()
                    .child(Topbar::new())
                    .child({
                        Routes::new()
                            .basename("/")
                            .child(Route::new().index().element(move |_, _| home.clone()))
                            .child(
                                Route::new()
                                    .path("library")
                                    .element(move |_, _| library.clone()),
                            )
                            .child(
                                Route::new()
                                    .path("settings")
                                    .element(move |_, _| settings.clone()),
                            )
                    }),
            )
    }
}
