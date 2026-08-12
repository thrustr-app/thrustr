use domain::{
    artwork::{ArtworkKind, Color},
    game::GameId,
};

mod color;
mod manager;
mod processing;
mod service;

pub use service::ArtworkService;

#[derive(Debug, Clone)]
pub struct ArtworkTask {
    pub game_id: GameId,
    pub url: String,
    pub kind: ArtworkKind,
    pub position: u32,
    pub quality: f32,
}

#[derive(Debug, Clone)]
pub struct ArtworkReady {
    pub game_id: GameId,
    pub hash: String,
    pub accent_color: Option<Color>,
}
