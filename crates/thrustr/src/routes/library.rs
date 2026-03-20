use crate::{extensions::SpawnTaskExt, globals::GameServiceExt};
use domain::GameListEntry;
use gpui::{
    Context, IntoElement, ParentElement, Render, Styled, Window, div, prelude::FluentBuilder,
};
use theme_manager::ThemeExt;

pub struct Library {
    games: Vec<GameListEntry>,
}

impl Library {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut page = Self { games: Vec::new() };

        page.refresh_games(cx);

        page
    }

    fn refresh_games(&mut self, cx: &mut Context<Self>) {
        let game_service = cx.game_service();

        cx.spawn_and_update(
            async move { game_service.list(0, 999999) },
            |library, result, _| {
                match result {
                    Ok(games) => {
                        library.games = games;
                    }
                    Err(e) => {
                        println!("{:?}", e);
                    }
                };
            },
        );
    }
}

impl Render for Library {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        div()
            .flex_grow()
            .text_color(theme.colors.accent)
            .map(move |this| {
                let mut this = this;
                for game in &self.games {
                    this = this.child(game.name.clone());
                }
                this
            })
    }
}
