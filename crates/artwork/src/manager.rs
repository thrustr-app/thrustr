use crate::{
    ArtworkReady, ArtworkTask, TaskKey,
    processing::{ProcessedArtwork, ProcessingError, process_task},
};
use config::paths::artwork_path;
use connectivity::ConnectivityManager;
use dashmap::{DashMap, DashSet};
use domain::artwork::{Artwork, ArtworkRepository};
use image::ImageError;
use lru::LruCache;
use rand::RngExt;
use reqwest::{Client, StatusCode};
use runtime::TokioHandle;
use std::{
    future::Future,
    hash::{DefaultHasher, Hash, Hasher},
    num::NonZeroUsize,
    path::Path,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{
    fs,
    sync::{OwnedSemaphorePermit, Semaphore, broadcast, mpsc},
    task::{JoinSet, spawn_blocking},
    time::sleep,
};
use tracing::{debug, error, warn};
use url::Url;

/// Tries within a single run, before the url is set aside for later.
const MAX_ATTEMPTS: u32 = 3;

/// A url may fail this many times before it is dropped for the rest of the session.
const MAX_URL_ATTEMPTS: u32 = 4;

/// The amount of times a throttled host can be retried
const MAX_HOST_THROTTLES: u32 = 5;
const DEFAULT_THROTTLE: Duration = Duration::from_secs(10);

/// A server may be incorrectly configured and return huge waiting times.
const MAX_THROTTLE: Duration = Duration::from_mins(10);

const MAX_TRACKED_URLS: usize = 1024;

const RECOVERY_JITTER_MAX: Duration = Duration::from_millis(500);

fn cooldown(attempts: u32) -> Duration {
    match attempts {
        0..=1 => Duration::from_secs(60),
        2 => Duration::from_secs(15 * 60),
        _ => Duration::from_secs(60 * 60),
    }
}

struct Cooldown {
    attempts: u32,
    until: Instant,
}

impl Cooldown {
    fn is_active(&self) -> bool {
        self.attempts >= MAX_URL_ATTEMPTS || Instant::now() < self.until
    }
}

fn url_key(url: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    hasher.finish()
}

struct Inner {
    inflight: DashSet<TaskKey>,
    cooldowns: Mutex<LruCache<u64, Cooldown>>,
    throttled_hosts: DashMap<String, Instant>,
    slots: Arc<Semaphore>,
}

impl Inner {
    async fn slot(&self) -> OwnedSemaphorePermit {
        self.slots
            .clone()
            .acquire_owned()
            .await
            .expect("slot semaphore should stay open")
    }

    fn is_cooling_down(&self, url: &str) -> bool {
        let mut cache = self
            .cooldowns
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        // `get` over `peek`, so the urls the library keeps asking for are the
        // ones that stay tracked.
        cache.get(&url_key(url)).is_some_and(Cooldown::is_active)
    }

    /// `None` when the URL has failed too many times and should not be retried again.
    fn cool_down(&self, url: &str) -> Option<Duration> {
        let key = url_key(url);
        let mut cache = self
            .cooldowns
            .lock()
            .unwrap_or_else(PoisonError::into_inner);

        let entry = cache.get_or_insert_mut(key, || Cooldown {
            attempts: 0,
            until: Instant::now(),
        });

        entry.attempts += 1;
        let delay = (entry.attempts < MAX_URL_ATTEMPTS).then(|| cooldown(entry.attempts));
        entry.until = Instant::now() + delay.unwrap_or_default();
        delay
    }

    fn throttle_host(&self, host: &str, delay: Duration) {
        let until = Instant::now() + delay;
        self.throttled_hosts
            .entry(host.to_string())
            .and_modify(|current| *current = (*current).max(until))
            .or_insert(until);
    }

    fn remaining_host_throttle(&self, host: &str) -> Option<Duration> {
        let until = *self.throttled_hosts.get(host)?;

        let remaining = until.checked_duration_since(Instant::now());
        if remaining.is_none() {
            self.throttled_hosts
                .remove_if(host, |_, until| *until <= Instant::now());
        }

        remaining
    }
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
            slots: Arc::new(Semaphore::new(max_concurrency)),
            inflight: DashSet::new(),
            cooldowns: Mutex::new(LruCache::new(
                NonZeroUsize::new(MAX_TRACKED_URLS)
                    .expect("tracked url capacity should not be zero"),
            )),
            throttled_hosts: DashMap::new(),
        });

        let client = Client::new();

        tokio_handle.spawn({
            let inner = inner.clone();
            let updates = updates.clone();
            let mut connectivity_watcher = connectivity.subscribe();

            async move {
                let mut join_set = JoinSet::new();

                loop {
                    while connectivity_watcher.current().is_offline() {
                        tokio::select! {
                            changed = connectivity_watcher.changed() => {
                                changed.expect("connectivity manager should outlive its watcher");
                            }
                            Some(_) = join_set.join_next() => {}
                        }
                    }

                    let task = tokio::select! {
                        task = receiver.recv() => match task {
                            Some(task) => task,
                            None => break,
                        },
                        Some(_) = join_set.join_next() => continue,
                    };

                    let permit = inner.slot().await;
                    join_set.spawn(run_with_retry(
                        task,
                        client.clone(),
                        connectivity.clone(),
                        repository.clone(),
                        updates.clone(),
                        inner.clone(),
                        permit,
                    ));
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
        if self.inner.is_cooling_down(&task.url) {
            return Ok(());
        }

        let key = task.key();
        if !self.inner.inflight.insert(key) {
            return Ok(());
        }

        self.sender.send(task).inspect_err(|_| {
            self.inner.inflight.remove(&key);
        })
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
    mut permit: OwnedSemaphorePermit,
) {
    let _guard = InflightGuard {
        inner: inner.clone(),
        key: task.key(),
    };

    // If the host cannot be obtained, building the request will fail anyway so
    // the key doesn't really matter.
    let host = host_of(&task.url).unwrap_or_else(|| task.url.clone());
    let mut attempts = 0;
    let mut throttles = 0;

    loop {
        permit = wait_out_throttle(&inner, &host, permit).await;

        // Another game may share this URL and have already set it aside.
        // Probably not a common case, but since the idea is to support metadata
        // providers, where different games (storefronts) can resolve to the same
        // URL, it doesn't hurt to keep it.
        if inner.is_cooling_down(&task.url) {
            return;
        }

        let error = match process_task(&task, client.clone()).await {
            Ok(processed) => {
                if let Err(e) = finalize(&task, processed, repository, &updates).await {
                    error!(game_id = %task.game_id, error = %e, "failed to persist artwork");
                }
                return;
            }
            Err(e) => e,
        };

        match Recovery::for_error(&error) {
            Recovery::ThrottleHost(delay) => {
                inner.throttle_host(&host, delay.min(MAX_THROTTLE));

                throttles += 1;
                if throttles >= MAX_HOST_THROTTLES || delay > MAX_THROTTLE {
                    warn!(%host, ?delay, throttles, "artwork task dropped while throttled");
                    return;
                }

                debug!(%host, ?delay, "artwork host is rate limiting us");
            }

            Recovery::ThrottleUrl => {
                set_aside(&inner, &task.url, &error);
                return;
            }

            recovery @ (Recovery::Reconnect | Recovery::Retry) => {
                attempts += 1;
                if attempts >= MAX_ATTEMPTS {
                    warn!(url = %task.url, attempts, error = %error, "artwork task gave up");
                    set_aside(&inner, &task.url, &error);
                    return;
                }

                debug!(url = %task.url, attempts, error = %error, "artwork task failed, retrying");

                if recovery == Recovery::Reconnect {
                    permit = without_slot(&inner, permit, async {
                        connectivity.report_error().await;
                        connectivity.wait_until_online().await;
                    })
                    .await;
                }

                // We hold the slot here since the wait is short and it prevents
                // the dispatcher from sending a new task at a host that just failed.
                sleep(backoff(attempts)).await;
            }
        }
    }
}

fn backoff(attempts: u32) -> Duration {
    Duration::from_secs(2u64.pow(attempts.saturating_sub(1))) + jitter()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Recovery {
    ThrottleHost(Duration),
    ThrottleUrl,
    Reconnect,
    Retry,
}

impl Recovery {
    fn for_error(error: &ProcessingError) -> Self {
        match error {
            ProcessingError::Request(e) if e.is_builder() || e.is_redirect() => Self::ThrottleUrl,
            ProcessingError::Request(e) if e.is_connect() || e.is_timeout() => Self::Reconnect,

            ProcessingError::Status {
                status,
                retry_after,
            } if *status == StatusCode::TOO_MANY_REQUESTS
                || (*status == StatusCode::SERVICE_UNAVAILABLE && retry_after.is_some()) =>
            {
                Self::ThrottleHost(retry_after.unwrap_or(DEFAULT_THROTTLE))
            }
            ProcessingError::Status { status, .. } if status.is_client_error() => Self::ThrottleUrl,

            // Only try again if the download might have been cut short.
            // These errors mean the file is broken in a way that retrying won't fix.
            ProcessingError::Decode(
                ImageError::Unsupported(_) | ImageError::Limits(_) | ImageError::Parameter(_),
            ) => Self::ThrottleUrl,

            ProcessingError::TooSmall { .. } => Self::ThrottleUrl,

            // A panic in the decoder repeats on the same bytes.
            ProcessingError::Task(e) if e.is_panic() => Self::ThrottleUrl,

            _ => Self::Retry,
        }
    }
}

fn set_aside(inner: &Inner, url: &str, error: &ProcessingError) {
    match inner.cool_down(url) {
        Some(delay) => debug!(url, ?delay, error = %error, "artwork url set aside"),
        None => debug!(url, error = %error, "artwork url will not be retried"),
    }
}

async fn wait_out_throttle(
    inner: &Inner,
    host: &str,
    mut permit: OwnedSemaphorePermit,
) -> OwnedSemaphorePermit {
    // The host may have been throttled again while this task was asleep.
    while let Some(remaining) = inner.remaining_host_throttle(host) {
        permit = without_slot(inner, permit, sleep(remaining + jitter())).await;
    }

    permit
}

/// Gives the slot back while waiting, so other tasks can use it.
/// Gets a new slot when the wait is over.
async fn without_slot(
    inner: &Inner,
    permit: OwnedSemaphorePermit,
    wait: impl Future<Output = ()>,
) -> OwnedSemaphorePermit {
    drop(permit);
    wait.await;
    inner.slot().await
}

fn host_of(url: &str) -> Option<String> {
    Url::parse(url).ok()?.host_str().map(str::to_string)
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

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }

    // Two games can share a cover, so the hash alone does not make the
    // temporary name unique.
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let temp = path.with_extension(format!("{}.tmp", SEQUENCE.fetch_add(1, Ordering::Relaxed)));

    let written = fs::write(&temp, bytes).await;
    let renamed = match written {
        Ok(()) => fs::rename(&temp, path).await,
        Err(e) => Err(e),
    };

    if let Err(e) = renamed {
        let _ = fs::remove_file(&temp).await;
        return Err(e.into());
    }

    Ok(())
}

/// A short random delay, so tasks that were held back by the same thing do not
/// all come back at once.
fn jitter() -> Duration {
    rand::rng().random_range(Duration::ZERO..RECOVERY_JITTER_MAX)
}
