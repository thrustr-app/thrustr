use crate::manager::ArtworkManager;
use connectivity::ConnectivityManager;
use domain::{
    artwork::{ArtworkKind, ArtworkRepository, Color},
    game::Game,
};
use std::sync::Arc;
use tokio::sync::broadcast;

const DEFAULT_QUALITY: f32 = 75.;
const DEFAULT_MAX_CONCURRENCY: usize = 4;

mod color;
mod manager;
mod processing;

#[derive(Debug, Clone)]
pub struct ArtworkTask {
    pub game_id: u64,
    pub url: String,
    pub kind: ArtworkKind,
    pub position: u32,
    pub quality: f32,
}

#[derive(Debug, Clone)]
pub struct ArtworkReady {
    pub game_id: u64,
    pub hash: String,
    pub accent_color: Option<Color>,
}

#[derive(Clone)]
pub struct ArtworkService {
    manager: Arc<ArtworkManager>,
}

impl ArtworkService {
    pub fn new(connectivity: ConnectivityManager, artwork: Arc<dyn ArtworkRepository>) -> Self {
        let manager = Arc::new(ArtworkManager::new(
            DEFAULT_MAX_CONCURRENCY,
            connectivity,
            artwork,
        ));
        Self { manager }
    }

    pub fn max_concurrent(&self) -> usize {
        self.manager.max_concurrent()
    }

    pub fn set_max_concurrency(&self, max: usize) {
        self.manager.set_max_concurrent(max);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ArtworkReady> {
        self.manager.subscribe()
    }

    pub fn enqueue_from_games(&self, games: &[Game]) {
        for game in games {
            if game.cover.is_some() {
                continue;
            }

            if let Some(cover_url) = &game.cover_url {
                self.enqueue_cover(game.id.into(), cover_url);
            }
        }
    }

    fn enqueue_cover(&self, game_id: u64, url: &str) {
        if url.is_empty() {
            return;
        }

        let task = ArtworkTask {
            game_id,
            url: url.to_string(),
            kind: ArtworkKind::Cover,
            position: 0,
            quality: DEFAULT_QUALITY,
        };

        if let Err(e) = self.manager.enqueue(task) {
            eprintln!("Failed to enqueue cover for game {}: {}", game_id, e);
        }
    }
}
