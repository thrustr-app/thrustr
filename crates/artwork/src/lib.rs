use crate::manager::ArtworkManager;
use connectivity::ConnectivityManager;
use domain::{
    artwork::{ArtworkKind, ArtworkRepository, Color},
    game::{GameId, GameRepository},
};
use std::{sync::Arc, time::Duration};
use tokio::sync::{Notify, broadcast};

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

struct Inner {
    manager: ArtworkManager,
    games: Arc<dyn GameRepository>,
    wakeup: Notify,
}

#[derive(Clone)]
pub struct ArtworkService(Arc<Inner>);

impl ArtworkService {
    pub fn new(
        connectivity: ConnectivityManager,
        artwork: Arc<dyn ArtworkRepository>,
        games: Arc<dyn GameRepository>,
    ) -> Self {
        let manager = ArtworkManager::new(DEFAULT_MAX_CONCURRENCY, connectivity, artwork);
        let service = Self(Arc::new(Inner {
            manager,
            games,
            wakeup: Notify::new(),
        }));

        tokio::spawn({
            let this = service.clone();
            async move {
                loop {
                    this.0.wakeup.notified().await;
                    this.backfill().await;
                }
            }
        });

        service
    }

    pub fn max_concurrent(&self) -> usize {
        self.0.manager.max_concurrent()
    }

    pub fn set_max_concurrency(&self, max: usize) {
        self.0.manager.set_max_concurrent(max);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ArtworkReady> {
        self.0.manager.subscribe()
    }

    pub fn pending(&self) -> usize {
        self.0.manager.pending()
    }

    pub fn trigger_backfill(&self) {
        self.0.wakeup.notify_one();
    }

    async fn backfill(&self) {
        let mut after = GameId::from(0);
        loop {
            let repo = self.0.games.clone();
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

        if let Err(e) = self.0.manager.enqueue(task) {
            eprintln!("Failed to enqueue cover for game {}: {}", game_id, e);
        }
    }
}
