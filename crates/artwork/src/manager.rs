use crate::{
    ArtworkReady, ArtworkTask,
    processing::{ProcessedArtwork, process_task, write_file},
};
use config::paths::artwork_path;
use connectivity::ConnectivityManager;
use dashmap::DashSet;
use domain::{
    artwork::{Artwork, ArtworkKind, ArtworkRepository},
    game::GameId,
};
use reqwest::Client;
use runtime::TokioHandle;
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    sync::{Notify, broadcast, mpsc},
    task::{JoinSet, spawn_blocking},
};

const MAX_ATTEMPTS: u32 = 3;
const RECOVERY_JITTER_MAX_MS: u64 = 500;

type TaskKey = (GameId, ArtworkKind, u32);

fn task_key(task: &ArtworkTask) -> TaskKey {
    (task.game_id, task.kind, task.position)
}

struct Inner {
    inflight: DashSet<TaskKey>,
    max_concurrent: AtomicUsize,
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
        max_concurrent: usize,
        connectivity: ConnectivityManager,
        artwork: Arc<dyn ArtworkRepository>,
    ) -> Self {
        let (sender, mut receiver) = mpsc::unbounded_channel::<ArtworkTask>();
        let (updates, _) = broadcast::channel(128);

        let inner = Arc::new(Inner {
            max_concurrent: AtomicUsize::new(max_concurrent),
            wakeup: Notify::new(),
            inflight: DashSet::new(),
        });

        let client = Client::new();

        tokio_handle.spawn({
            let inner = inner.clone();
            let updates = updates.clone();
            let mut connectivity_rx = connectivity.subscribe();

            async move {
                let mut join_set = JoinSet::new();

                loop {
                    while join_set.try_join_next().is_some() {}

                    while connectivity_rx.borrow_and_update().is_offline() {
                        tokio::select! {
                            _ = connectivity_rx.changed() => {}
                            Some(_) = join_set.join_next() => {}
                        }
                    }

                    if join_set.len() >= inner.max_concurrent.load(Ordering::Acquire) {
                        tokio::select! {
                            Some(_) = join_set.join_next() => {}
                            _ = inner.wakeup.notified() => {}
                        }
                        continue;
                    }

                    tokio::select! {
                        task = receiver.recv() => match task {
                            Some(task) => {
                                join_set.spawn(run_with_retry(task, client.clone(), connectivity.clone(), artwork.clone(), updates.clone(), inner.clone()));
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

    pub fn max_concurrent(&self) -> usize {
        self.inner.max_concurrent.load(Ordering::Acquire)
    }

    pub fn set_max_concurrent(&self, max: usize) {
        self.inner.max_concurrent.store(max, Ordering::Release);
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
    artwork: Arc<dyn ArtworkRepository>,
    updates: broadcast::Sender<ArtworkReady>,
    inner: Arc<Inner>,
) {
    let _guard = InflightGuard {
        inner,
        key: task_key(&task),
    };

    let mut attempts = 0;
    loop {
        match process_task(task.clone(), client.clone()).await {
            Ok(processed) => {
                let game_id = task.game_id;
                if let Err(e) = finalize(task, processed, artwork, &updates).await {
                    eprintln!("Failed to persist cover for game {}: {}", game_id, e);
                }
                return;
            }
            Err(e) => {
                attempts += 1;
                if attempts >= MAX_ATTEMPTS {
                    eprintln!(
                        "Task {} failed after {} attempts, giving up: {}",
                        task.url, MAX_ATTEMPTS, e
                    );
                    return;
                }

                if is_network_error(&e) {
                    connectivity.report_error();
                    connectivity.wait_until_online().await;
                }

                let delay = Duration::from_secs(2u64.pow(attempts - 1));
                eprintln!(
                    "Task {} attempt {}/{} failed, retrying in {:?}: {}",
                    task.url, attempts, MAX_ATTEMPTS, delay, e
                );
                tokio::time::sleep(delay).await;
                jitter().await;
            }
        }
    }
}

async fn finalize(
    task: ArtworkTask,
    processed: ProcessedArtwork,
    artwork: Arc<dyn ArtworkRepository>,
    updates: &broadcast::Sender<ArtworkReady>,
) -> anyhow::Result<()> {
    let path = artwork_path(&processed.hash, "webp");
    write_file(&path, &processed.bytes).await?;

    let hash = processed.hash.clone();
    let accent_color = processed.color;
    let record = Artwork {
        hash: processed.hash,
        kind: task.kind,
        position: task.position,
        accent_color: processed.color,
    };
    spawn_blocking(move || artwork.insert(task.game_id, &record)).await??;

    let _ = updates.send(ArtworkReady {
        game_id: task.game_id,
        hash,
        accent_color,
    });
    Ok(())
}

fn is_network_error(e: &anyhow::Error) -> bool {
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
