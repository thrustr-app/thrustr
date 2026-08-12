use crate::{
    ArtworkReady, ArtworkTask,
    processing::{ProcessedArtwork, process_task},
};
use config::paths::artwork_path;
use connectivity::ConnectivityManager;
use dashmap::DashSet;
use domain::{
    artwork::{Artwork, ArtworkKind, ArtworkRepository},
    game::GameId,
};
use reqwest::{Client, StatusCode};
use runtime::TokioHandle;
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    fs,
    sync::{Notify, broadcast, mpsc},
    task::{JoinSet, spawn_blocking},
};
use tracing::{error, warn};

const MAX_ATTEMPTS: u32 = 3;
const RECOVERY_JITTER_MAX_MS: u64 = 500;

type TaskKey = (GameId, ArtworkKind, u32);

fn task_key(task: &ArtworkTask) -> TaskKey {
    (task.game_id, task.kind, task.position)
}

struct Inner {
    inflight: DashSet<TaskKey>,
    max_concurrency: AtomicUsize,
    wakeup: Notify,
}

struct InflightGuard {
    inner: Arc<Inner>,
    key: TaskKey,
}

impl Drop for InflightGuard {
    fn drop(&mut self) {
        self.inner.inflight.remove(&self.key);
    }
}

pub struct ArtworkManager {
    sender: mpsc::UnboundedSender<ArtworkTask>,
    inner: Arc<Inner>,
    updates: broadcast::Sender<ArtworkReady>,
}

impl ArtworkManager {
    pub fn new(
        tokio_handle: TokioHandle,
        max_concurrency: usize,
        connectivity: ConnectivityManager,
        repository: Arc<dyn ArtworkRepository>,
    ) -> Self {
        let (sender, mut receiver) = mpsc::unbounded_channel::<ArtworkTask>();
        let (updates, _) = broadcast::channel(128);

        let inner = Arc::new(Inner {
            max_concurrency: AtomicUsize::new(max_concurrency),
            wakeup: Notify::new(),
            inflight: DashSet::new(),
        });

        let client = Client::new();

        tokio_handle.spawn({
            let inner = inner.clone();
            let updates = updates.clone();
            let mut connectivity_watcher = connectivity.subscribe();

            async move {
                let mut join_set = JoinSet::new();

                loop {
                    while join_set.try_join_next().is_some() {}

                    while connectivity_watcher.current().is_offline() {
                        tokio::select! {
                            _ = connectivity_watcher.changed() => {}
                            Some(_) = join_set.join_next() => {}
                        }
                    }

                    if join_set.len() >= inner.max_concurrency.load(Ordering::Acquire) {
                        tokio::select! {
                            Some(_) = join_set.join_next() => {}
                            _ = inner.wakeup.notified() => {}
                        }
                        continue;
                    }

                    tokio::select! {
                        task = receiver.recv() => match task {
                            Some(task) => {
                                join_set.spawn(run_with_retry(task, client.clone(), connectivity.clone(), repository.clone(), updates.clone(), inner.clone()));
                            }
                            None => break,
                        },
                        _ = inner.wakeup.notified() => {}
                    }
                }

                while join_set.join_next().await.is_some() {}
            }
        });

        Self {
            sender,
            inner,
            updates,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ArtworkReady> {
        self.updates.subscribe()
    }

    pub fn enqueue(&self, task: ArtworkTask) -> Result<(), mpsc::error::SendError<ArtworkTask>> {
        let key = task_key(&task);
        if !self.inner.inflight.insert(key) {
            return Ok(());
        }

        self.sender.send(task).inspect_err(|_| {
            self.inner.inflight.remove(&key);
        })
    }

    pub fn max_concurrency(&self) -> usize {
        self.inner.max_concurrency.load(Ordering::Acquire)
    }

    pub fn set_max_concurrency(&self, max: usize) {
        self.inner.max_concurrency.store(max, Ordering::Release);
        self.inner.wakeup.notify_one();
    }

    pub fn pending(&self) -> usize {
        self.inner.inflight.len()
    }
}

async fn run_with_retry(
    task: ArtworkTask,
    client: Client,
    connectivity: ConnectivityManager,
    repository: Arc<dyn ArtworkRepository>,
    updates: broadcast::Sender<ArtworkReady>,
    inner: Arc<Inner>,
) {
    let _guard = InflightGuard {
        inner,
        key: task_key(&task),
    };

    let mut attempts = 0;
    loop {
        match process_task(&task, client.clone()).await {
            Ok(processed) => {
                let game_id = task.game_id;
                if let Err(e) = finalize(&task, processed, repository, &updates).await {
                    error!(%game_id, error = %e, "failed to persist cover");
                }
                return;
            }
            Err(e) => {
                attempts += 1;
                if !is_retryable(&e) {
                    warn!(url = %task.url, error = %e, "artwork task failed permanently");
                    return;
                }

                if attempts >= MAX_ATTEMPTS {
                    warn!(url = %task.url, attempts, error = %e, "artwork task gave up");
                    return;
                }

                if is_offline_error(&e) {
                    connectivity.report_error().await;
                    connectivity.wait_until_online().await;
                }

                warn!(url = %task.url, attempts, error = %e, "artwork task failed, retrying");
                tokio::time::sleep(Duration::from_secs(2u64.pow(attempts - 1))).await;
                jitter().await;
            }
        }
    }
}

async fn finalize(
    task: &ArtworkTask,
    processed: ProcessedArtwork,
    repository: Arc<dyn ArtworkRepository>,
    updates: &broadcast::Sender<ArtworkReady>,
) -> anyhow::Result<()> {
    let ProcessedArtwork { bytes, hash, color } = processed;
    write_artwork(&artwork_path(&hash, "webp"), &bytes).await?;

    let game_id = task.game_id;
    let record = Artwork {
        hash: hash.clone(),
        kind: task.kind,
        position: task.position,
        accent_color: color,
    };
    spawn_blocking(move || repository.insert(game_id, &record)).await??;

    let _ = updates.send(ArtworkReady {
        game_id,
        hash,
        accent_color: color,
    });
    Ok(())
}

async fn write_artwork(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    if fs::try_exists(path).await? {
        return Ok(());
    }

    // Two games can share a cover, so the hash alone does not make the
    // temporary name unique.
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let temp = path.with_extension(format!("{}.tmp", SEQUENCE.fetch_add(1, Ordering::Relaxed)));

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }

    fs::write(&temp, bytes).await?;
    if let Err(e) = fs::rename(&temp, path).await {
        let _ = fs::remove_file(&temp).await;
        return Err(e.into());
    }

    Ok(())
}

fn is_retryable(e: &anyhow::Error) -> bool {
    let Some(e) = e.downcast_ref::<reqwest::Error>() else {
        return false;
    };

    e.is_connect()
        || e.is_timeout()
        || e.is_request()
        || e.status().is_some_and(|status| {
            status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS
        })
}

fn is_offline_error(e: &anyhow::Error) -> bool {
    e.downcast_ref::<reqwest::Error>()
        .is_some_and(|e| e.is_connect() || e.is_timeout())
}

/// Sleeps for a random short duration to avoid multiple waiting tasks from retrying at the same time.
async fn jitter() {
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let ms = seed % RECOVERY_JITTER_MAX_MS as u32;
    tokio::time::sleep(Duration::from_millis(ms as u64)).await;
}
