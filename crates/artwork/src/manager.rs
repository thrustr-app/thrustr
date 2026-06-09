use crate::{
    ArtworkTask,
    processing::{ProcessedArtwork, process_task, write_file},
};
use config::paths::artwork_path;
use connectivity::ConnectivityManager;
use domain::artwork::{Artwork, ArtworkRepository};
use reqwest::Client;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    sync::{Notify, mpsc},
    task::{JoinSet, spawn_blocking},
};

const MAX_ATTEMPTS: u32 = 3;
const RECOVERY_JITTER_MAX_MS: u64 = 500;

struct Inner {
    pending: AtomicUsize,
    active: AtomicUsize,
    max_concurrent: AtomicUsize,
    paused: AtomicBool,
    wakeup: Notify,
}

pub struct ArtworkManager {
    sender: mpsc::UnboundedSender<ArtworkTask>,
    inner: Arc<Inner>,
}

impl ArtworkManager {
    pub fn new(
        max_concurrent: usize,
        connectivity: ConnectivityManager,
        artwork: Arc<dyn ArtworkRepository>,
    ) -> Self {
        let (sender, mut receiver) = mpsc::unbounded_channel::<ArtworkTask>();

        let inner = Arc::new(Inner {
            pending: AtomicUsize::new(0),
            active: AtomicUsize::new(0),
            max_concurrent: AtomicUsize::new(max_concurrent),
            paused: AtomicBool::new(false),
            wakeup: Notify::new(),
        });

        let client = Client::new();

        tokio::spawn({
            let inner = inner.clone();
            let mut connectivity_rx = connectivity.subscribe();

            async move {
                let mut join_set = JoinSet::new();

                loop {
                    while join_set.try_join_next().is_some() {
                        inner.pending.fetch_sub(1, Ordering::Relaxed);
                        inner.active.fetch_sub(1, Ordering::Relaxed);
                    }

                    while connectivity_rx.borrow_and_update().is_offline() {
                        tokio::select! {
                            _ = connectivity_rx.changed() => {}
                            Some(_) = join_set.join_next() => {
                                inner.pending.fetch_sub(1, Ordering::Relaxed);
                                inner.active.fetch_sub(1, Ordering::Relaxed);
                            }
                        }
                    }

                    if inner.paused.load(Ordering::Acquire) {
                        tokio::select! {
                            _ = inner.wakeup.notified() => {},
                            Some(_) = join_set.join_next() => {
                                inner.pending.fetch_sub(1, Ordering::Relaxed);
                                inner.active.fetch_sub(1, Ordering::Relaxed);
                            }
                        }
                        continue;
                    }

                    if inner.active.load(Ordering::Acquire)
                        >= inner.max_concurrent.load(Ordering::Acquire)
                    {
                        tokio::select! {
                            Some(_) = join_set.join_next() => {
                                inner.pending.fetch_sub(1, Ordering::Relaxed);
                                inner.active.fetch_sub(1, Ordering::Relaxed);
                            }
                            _ = inner.wakeup.notified() => {}
                        }
                        continue;
                    }

                    tokio::select! {
                        task = receiver.recv() => match task {
                            Some(task) => {
                                join_set.spawn(run_with_retry(task, client.clone(), connectivity.clone(), artwork.clone()));
                                inner.active.fetch_add(1, Ordering::Relaxed);
                            }
                            None => break,
                        },
                        _ = inner.wakeup.notified() => {}
                    }
                }

                while join_set.join_next().await.is_some() {
                    inner.pending.fetch_sub(1, Ordering::Relaxed);
                    inner.active.fetch_sub(1, Ordering::Relaxed);
                }
            }
        });

        Self { sender, inner }
    }

    pub fn enqueue(&self, task: ArtworkTask) -> Result<(), mpsc::error::SendError<ArtworkTask>> {
        self.inner.pending.fetch_add(1, Ordering::Relaxed);
        self.sender.send(task).inspect_err(|_| {
            self.inner.pending.fetch_sub(1, Ordering::Relaxed);
        })
    }

    pub fn _is_paused(&self) -> bool {
        self.inner.paused.load(Ordering::Acquire)
    }

    pub fn _pause(&self) {
        self.inner.paused.store(true, Ordering::Release);
    }

    pub fn _resume(&self) {
        self.inner.paused.store(false, Ordering::Release);
        self.inner.wakeup.notify_one();
    }

    pub fn max_concurrent(&self) -> usize {
        self.inner.max_concurrent.load(Ordering::Acquire)
    }

    pub fn set_max_concurrent(&self, max: usize) {
        self.inner.max_concurrent.store(max, Ordering::Release);
        self.inner.wakeup.notify_one();
    }

    pub fn _active(&self) -> usize {
        self.inner.active.load(Ordering::Acquire)
    }

    pub fn _pending(&self) -> usize {
        self.inner.pending.load(Ordering::Acquire)
    }
}

async fn run_with_retry(
    task: ArtworkTask,
    client: Client,
    connectivity: ConnectivityManager,
    artwork: Arc<dyn ArtworkRepository>,
) {
    let mut attempts = 0;
    loop {
        match process_task(task.clone(), client.clone()).await {
            Ok(processed) => {
                let game_id = task.game_id;
                if let Err(e) = finalize(task, processed, artwork).await {
                    eprintln!("Failed to persist cover for game {}: {}", game_id, e);
                }
                return;
            }
            Err(ref e) if is_network_error(e) => {
                connectivity.report_error();
                connectivity.wait_until_online().await;
                jitter().await;
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
                let delay = Duration::from_secs(2u64.pow(attempts - 1));
                eprintln!(
                    "Task {} attempt {}/{} failed, retrying in {:?}: {}",
                    task.url, attempts, MAX_ATTEMPTS, delay, e
                );
                tokio::time::sleep(delay).await;
            }
        }
    }
}

/// Writes the processed image to its content-addressed path, then persists the artwork
/// metadata. The DB row is only written after the file exists on disk, so the table
/// never references a missing file.
async fn finalize(
    task: ArtworkTask,
    processed: ProcessedArtwork,
    artwork: Arc<dyn ArtworkRepository>,
) -> anyhow::Result<()> {
    let path = artwork_path(&processed.hash, "webp");
    write_file(&path, &processed.bytes).await?;

    let record = Artwork {
        hash: processed.hash,
        kind: task.kind,
        position: task.position,
        accent_color: processed.color,
    };
    spawn_blocking(move || artwork.insert(task.game_id.into(), &record)).await??;

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
