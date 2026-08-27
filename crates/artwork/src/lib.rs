use domain::{
    artwork::{ArtworkKind, Color},
    game::GameId,
};

mod color;
mod manager;
mod processing;
mod service;

pub use service::ArtworkService;

type TaskKey = (GameId, ArtworkKind, u32);

#[derive(Debug, Clone)]
pub struct ArtworkTask {
    pub game_id: GameId,
    pub url: String,
    pub kind: ArtworkKind,
    pub position: u32,
    pub quality: f32,
}

impl ArtworkTask {
    fn key(&self) -> TaskKey {
        (self.game_id, self.kind, self.position)
    }
}

#[derive(Debug, Clone)]
pub struct ArtworkReady {
    pub game_id: GameId,
    pub hash: String,
    pub accent: Option<Color>,
}
