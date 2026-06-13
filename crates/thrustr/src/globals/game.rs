use domain::game::GameRepository;
use game::GameService;
use gpui::{App, Global};
use std::sync::Arc;

pub(super) struct GameServiceGlobal(GameService);

impl Global for GameServiceGlobal {}

pub(super) fn init(cx: &mut App, repo: Arc<dyn GameRepository>) {
    let service = GameService::new(repo);
    cx.set_global(GameServiceGlobal(service));
}

pub trait GameServiceExt {
    fn game_service(&self) -> GameService;
}

impl GameServiceExt for App {
    fn game_service(&self) -> GameService {
        self.global::<GameServiceGlobal>().0.clone()
    }
}
