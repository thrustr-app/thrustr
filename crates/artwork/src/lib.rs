use crate::manager::ArtworkManager;
use connectivity::ConnectivityManager;
use domain::{
    artwork::{ArtworkKind, ArtworkRepository, Color},
    game::{Game, GameId, GameRepository},
};
use std::{sync::Arc, time::Duration};
use tokio::sync::broadcast;

const DEFAULT_QUALITY: f32 = 75.;
const DEFAULT_MAX_CONCURRENCY: usize = 4;

const BACKFILL_PAGE: usize = 500;
const BACKFILL_PENDING_HIGH: usize = 1_000;
const BACKFILL_BACKOFF: Duration = Duration::from_millis(250);

mod color;
mod manager;
mod processing;

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

#[derive(Clone)]
pub struct ArtworkService {
    manager: Arc<ArtworkManager>,
    games: Arc<dyn GameRepository>,
}

impl ArtworkService {
    pub fn new(
        connectivity: ConnectivityManager,
        artwork: Arc<dyn ArtworkRepository>,
        games: Arc<dyn GameRepository>,
    ) -> Self {
        let manager = Arc::new(ArtworkManager::new(
            DEFAULT_MAX_CONCURRENCY,
            connectivity,
            artwork,
        ));
        Self { manager, games }
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
                self.enqueue_cover(game.id, cover_url);
            }
        }
    }

    pub fn pending(&self) -> usize {
        self.manager.pending()
    }

    pub async fn backfill(&self) {
        let mut after = GameId::from(0);
        loop {
            let repo = self.games.clone();
            let cursor = after;
            let batch = match tokio::task::spawn_blocking(move || {
                repo.games_missing_artwork(ArtworkKind::Cover, cursor, BACKFILL_PAGE)
            })
            .await
            {
                Ok(Ok(batch)) => batch,
                Ok(Err(e)) => {
                    eprintln!("Artwork backfill query failed: {e}");
                    return;
                }
                Err(e) => {
                    eprintln!("Artwork backfill task failed: {e}");
                    return;
                }
            };

            if batch.is_empty() {
                return;
            }

            for (id, url) in &batch {
                self.enqueue_cover(*id, url);
                after = *id;
            }

            while self.pending() >= BACKFILL_PENDING_HIGH {
                tokio::time::sleep(BACKFILL_BACKOFF).await;
            }
        }
    }

    pub fn enqueue_cover(&self, game_id: GameId, url: &str) {
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
