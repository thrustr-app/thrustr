use crate::manager::ImageManager;
use config::paths::cover_path;
use connectivity::ConnectivityManager;
use domain::game::Game;
use std::{path::PathBuf, sync::Arc};

const DEFAULT_QUALITY: f32 = 75.;
const DEFAULT_MAX_CONCURRENCY: usize = 4;

mod manager;
mod processing;

#[derive(Debug, Clone)]
pub struct ImageTask {
    pub url: String,
    pub path: PathBuf,
    pub quality: f32,
}

#[derive(Clone)]
pub struct ImageService {
    manager: Arc<ImageManager>,
}

impl ImageService {
    pub fn new(connectivity: ConnectivityManager) -> Self {
        let manager = Arc::new(ImageManager::new(DEFAULT_MAX_CONCURRENCY, connectivity));
        Self { manager }
    }

    pub fn max_concurrent(&self) -> usize {
        self.manager.max_concurrent()
    }

    pub fn set_max_concurrency(&self, max: usize) {
        self.manager.set_max_concurrent(max);
    }

    pub fn enqueue_from_games(&self, games: &[Game]) {
        for game in games {
            self.enqueue_cover(game.id.into(), &game.cover_url);
        }
    }

    fn enqueue_cover(&self, id: u64, url: &str) {
        if url.is_empty() {
            return;
        }

        let path = cover_path(id, "webp");

        if path.exists() {
            return;
        }

        let task = ImageTask {
            url: url.to_string(),
            path,
            quality: DEFAULT_QUALITY,
        };

        if let Err(e) = self.manager.enqueue(task) {
            eprintln!("Failed to enqueue cover for game {}: {}", id, e);
        }
    }
}
