use crate::{ArtworkReady, ArtworkTask, manager::ArtworkManager};
use config::paths::artwork_dir;
use connectivity::ConnectivityManager;
use domain::{
    artwork::{ArtworkKind, ArtworkRepository},
    game::{GameId, GameRepository},
};
use runtime::TokioHandle;
use std::{sync::Arc, time::Duration};
use tokio::sync::{Notify, broadcast};
use tracing::error;

const DEFAULT_QUALITY: f32 = 75.;
const DEFAULT_MAX_CONCURRENCY: usize = 4;

const BACKFILL_PAGE: usize = 500;
const BACKFILL_PENDING_HIGH: usize = 1_000;
const BACKFILL_BACKOFF: Duration = Duration::from_millis(250);

struct Inner {
    manager: ArtworkManager,
    games: Arc<dyn GameRepository>,
    wakeup: Notify,
}

#[derive(Clone)]
pub struct ArtworkService(Arc<Inner>);

impl ArtworkService {
    pub fn new(
        tokio_handle: TokioHandle,
        connectivity: ConnectivityManager,
        artwork: Arc<dyn ArtworkRepository>,
        games: Arc<dyn GameRepository>,
    ) -> Self {
        let manager = ArtworkManager::new(
            tokio_handle.clone(),
            DEFAULT_MAX_CONCURRENCY,
            artwork_dir(),
            connectivity,
            artwork,
        );
        let service = Self(Arc::new(Inner {
            manager,
            games,
            wakeup: Notify::new(),
        }));

        tokio_handle.spawn({
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
                repo.list_missing_artwork(ArtworkKind::Cover, cursor, BACKFILL_PAGE)
            })
            .await
            {
                Ok(Ok(batch)) => batch,
                Ok(Err(e)) => {
                    error!(error = %e, "artwork backfill query failed");
                    return;
                }
                Err(e) => {
                    error!(error = %e, "artwork backfill task failed");
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
            error!(%game_id, error = %e, "failed to enqueue cover");
        }
    }
}
