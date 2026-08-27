use crate::{
    ArtworkReady, ArtworkTask, TaskKey,
    processing::{ProcessedArtwork, ProcessingError, process_task},
};
use config::paths::artwork_path_in;
use dashmap::{DashMap, DashSet};
use domain::artwork::{Artwork, ArtworkRepository};
use image::ImageError;
use lru::LruCache;
use net::{ConnectivityManager, backoff, host_of, jitter};
use reqwest::{Client, StatusCode};
use runtime::TokioHandle;
use std::{
    future::Future,
    hash::{DefaultHasher, Hash, Hasher},
    num::NonZeroUsize,
    path::{Path, PathBuf},
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
    artwork_dir: PathBuf,
}

impl Inner {
    fn new(max_concurrency: usize, artwork_dir: PathBuf) -> Self {
        Self {
            artwork_dir,
            slots: Arc::new(Semaphore::new(max_concurrency)),
            inflight: DashSet::new(),
            cooldowns: Mutex::new(LruCache::new(
                NonZeroUsize::new(MAX_TRACKED_URLS)
                    .expect("tracked url capacity should not be zero"),
            )),
            throttled_hosts: DashMap::new(),
        }
    }

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
        artwork_dir: PathBuf,
        connectivity: ConnectivityManager,
        repository: Arc<dyn ArtworkRepository>,
    ) -> Self {
        let (sender, mut receiver) = mpsc::unbounded_channel::<ArtworkTask>();
        let (updates, _) = broadcast::channel(128);

        let inner = Arc::new(Inner::new(max_concurrency, artwork_dir));

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
                if let Err(e) =
                    finalize(&task, processed, &inner.artwork_dir, repository, &updates).await
                {
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

async fn finalize(
    task: &ArtworkTask,
    processed: ProcessedArtwork,
    artwork_dir: &Path,
    repository: Arc<dyn ArtworkRepository>,
    updates: &broadcast::Sender<ArtworkReady>,
) -> anyhow::Result<()> {
    let ProcessedArtwork { bytes, hash, color } = processed;
    write_artwork(&artwork_path_in(artwork_dir, &hash, "webp")?, &bytes).await?;

    let game_id = task.game_id;
    let record = Artwork {
        hash: hash.clone(),
        kind: task.kind,
        position: task.position,
        accent: color,
    };
    spawn_blocking(move || repository.insert(game_id, &record)).await??;

    let _ = updates.send(ArtworkReady {
        game_id,
        hash,
        accent: color,
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

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{
        artwork::{ArtworkKind, Color},
        game::GameId,
    };
    use image::{
        DynamicImage, ImageFormat, Rgb, RgbImage,
        error::{
            ImageFormatHint, LimitError, LimitErrorKind, ParameterError, ParameterErrorKind,
            UnsupportedError, UnsupportedErrorKind,
        },
    };
    use net::ConnectivityConfig;
    use reqwest::header::RETRY_AFTER;
    use std::{io, io::Cursor};
    use tempfile::TempDir;
    use tokio::time::timeout;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    const URL: &str = "https://images.example.com/cover.png";
    const OTHER_URL: &str = "https://images.example.com/other.png";
    const HOST: &str = "images.example.com";

    const MINUTE: Duration = Duration::from_mins(1);

    fn task(game_id: u64, position: u32, url: &str) -> ArtworkTask {
        ArtworkTask {
            game_id: GameId::from(game_id),
            url: url.to_string(),
            kind: ArtworkKind::Cover,
            position,
            quality: 75.,
        }
    }

    fn inner() -> Inner {
        Inner::new(1, PathBuf::new())
    }

    #[track_caller]
    fn check_cool_down(inner: &Inner, url: &str, expected: Option<Duration>) {
        assert_eq!(inner.cool_down(url), expected, "cooling down {url}");
    }

    #[test]
    fn a_failing_url_waits_longer_after_every_failure() {
        let inner = inner();

        for expected in [MINUTE, 15 * MINUTE, 60 * MINUTE] {
            check_cool_down(&inner, URL, Some(expected));
            assert!(inner.is_cooling_down(URL), "{URL} should be set aside");
        }
    }

    #[test]
    fn a_url_that_keeps_failing_is_dropped_for_good() {
        let inner = inner();
        for _ in 0..MAX_URL_ATTEMPTS {
            inner.cool_down(URL);
        }

        check_cool_down(&inner, URL, None);
        assert!(
            inner.is_cooling_down(URL),
            "{URL} should stay set aside once it has consumed all the attempts"
        );
    }

    #[test]
    fn cool_downs_are_per_url() {
        let inner = inner();
        inner.cool_down(URL);

        assert!(!inner.is_cooling_down(OTHER_URL));
        check_cool_down(&inner, OTHER_URL, Some(MINUTE));
    }

    #[test]
    fn the_oldest_cooldown_is_evicted_when_capacity_is_exceeded() {
        let inner = inner();
        inner.cool_down(URL);

        for i in 0..MAX_TRACKED_URLS {
            inner.cool_down(&format!("https://images.example.com/{i}.png"));
        }

        assert!(
            !inner.is_cooling_down(URL),
            "the oldest url should have been forgotten"
        );
    }

    #[test]
    fn a_host_waits_out_its_longest_throttle() {
        let inner = inner();
        inner.throttle_host(HOST, MINUTE);
        inner.throttle_host(HOST, Duration::from_secs(1));

        let remaining = inner
            .remaining_host_throttle(HOST)
            .expect("the host should still be throttled");
        assert!(
            remaining > Duration::from_secs(30),
            "the shorter throttle should not have replaced the longer one, {remaining:?} left"
        );
    }

    #[test]
    fn throttles_are_forgotten_once_they_expire() {
        let inner = inner();
        inner.throttle_host(HOST, Duration::ZERO);

        assert_eq!(inner.remaining_host_throttle(HOST), None);
        assert!(
            inner.throttled_hosts.is_empty(),
            "an expired throttle should not be kept around"
        );
    }

    #[test]
    fn throttles_are_per_host() {
        let inner = inner();
        inner.throttle_host(HOST, MINUTE);

        assert_eq!(inner.remaining_host_throttle("other.example.com"), None);
    }

    #[track_caller]
    fn check_recovery(error: ProcessingError, expected: Recovery) {
        assert_eq!(
            Recovery::for_error(&error),
            expected,
            "recovering from {error:?}"
        );
    }

    fn status(code: u16, retry_after: Option<Duration>) -> ProcessingError {
        ProcessingError::Status {
            status: StatusCode::from_u16(code).expect("the status code should be valid"),
            retry_after,
        }
    }

    #[test]
    fn rate_limits_throttle_the_whole_host() {
        let retry_after = Duration::from_secs(30);

        check_recovery(status(429, None), Recovery::ThrottleHost(DEFAULT_THROTTLE));
        check_recovery(
            status(429, Some(retry_after)),
            Recovery::ThrottleHost(retry_after),
        );
        check_recovery(
            status(503, Some(retry_after)),
            Recovery::ThrottleHost(retry_after),
        );
    }

    #[test]
    fn answers_that_will_not_change_set_the_url_aside() {
        let unsupported = UnsupportedError::from_format_and_kind(
            ImageFormatHint::Unknown,
            UnsupportedErrorKind::Format(ImageFormatHint::Unknown),
        );

        for error in [
            status(404, None),
            status(403, None),
            status(400, Some(Duration::from_secs(30))),
            ProcessingError::Decode(ImageError::Unsupported(unsupported)),
            ProcessingError::Decode(ImageError::Limits(LimitError::from_kind(
                LimitErrorKind::DimensionError,
            ))),
            ProcessingError::Decode(ImageError::Parameter(ParameterError::from_kind(
                ParameterErrorKind::DimensionMismatch,
            ))),
            ProcessingError::TooSmall {
                width: 1,
                height: 1,
            },
        ] {
            check_recovery(error, Recovery::ThrottleUrl);
        }
    }

    #[test]
    fn answers_that_might_change_are_retried() {
        check_recovery(status(500, None), Recovery::Retry);
        check_recovery(status(503, None), Recovery::Retry);
        check_recovery(
            ProcessingError::Decode(ImageError::IoError(io::Error::from(
                io::ErrorKind::UnexpectedEof,
            ))),
            Recovery::Retry,
        );
    }

    #[tokio::test]
    async fn a_panicked_task_is_set_aside() {
        let panicked = tokio::spawn(async { panic!("the decoder gave up") })
            .await
            .expect_err("the task should have panicked");
        check_recovery(ProcessingError::Task(panicked), Recovery::ThrottleUrl);
    }

    #[tokio::test]
    async fn a_cancelled_task_is_retried() {
        let handle = tokio::spawn(std::future::pending::<()>());
        handle.abort();
        let cancelled = handle.await.expect_err("the task should have been aborted");
        check_recovery(ProcessingError::Task(cancelled), Recovery::Retry);
    }

    #[tokio::test]
    async fn a_url_that_cannot_be_requested_is_set_aside() {
        let error = Client::new()
            .get("not a url")
            .build()
            .expect_err("the request should not build");

        check_recovery(ProcessingError::Request(error), Recovery::ThrottleUrl);
    }

    fn parked_manager(artwork_dir: &Path) -> ArtworkManager {
        manager(artwork_dir, 0, Recorder::new())
    }

    #[tokio::test]
    async fn duplicate_artwork_tasks_are_only_queued_once() {
        let dir = temp_dir();
        let manager = parked_manager(dir.path());

        manager
            .enqueue(task(1, 0, URL))
            .expect("the task should queue");
        manager
            .enqueue(task(1, 0, URL))
            .expect("a skipped task is not an error");
        assert_eq!(
            manager.pending(),
            1,
            "the duplicate artwork task was queued twice"
        );

        manager
            .enqueue(task(1, 1, OTHER_URL))
            .expect("the task should queue");
        manager
            .enqueue(task(2, 0, URL))
            .expect("the task should queue");
        assert_eq!(
            manager.pending(),
            3,
            "distinct artwork tasks should all be queued"
        );
    }

    #[tokio::test]
    async fn urls_that_are_cooling_down_are_not_queued() {
        let dir = temp_dir();
        let manager = parked_manager(dir.path());
        manager.inner.cool_down(URL);

        manager
            .enqueue(task(1, 0, URL))
            .expect("a skipped task is not an error");

        assert_eq!(manager.pending(), 0);
    }

    fn temp_dir() -> TempDir {
        TempDir::new().expect("the temporary directory should be created")
    }

    fn files(dir: &TempDir) -> Vec<String> {
        files_under(dir.path())
            .iter()
            .map(|path| {
                path.file_name()
                    .expect("a file should have a name")
                    .to_string_lossy()
                    .into_owned()
            })
            .collect()
    }

    fn files_under(dir: &Path) -> Vec<PathBuf> {
        let mut found: Vec<_> = std::fs::read_dir(dir)
            .expect("the directory should be readable")
            .flat_map(|entry| {
                let path = entry.expect("the entry should be readable").path();
                if path.is_dir() {
                    files_under(&path)
                } else {
                    vec![path]
                }
            })
            .collect();

        found.sort();
        found
    }

    #[tokio::test]
    async fn artwork_lands_on_disk_under_its_own_name() {
        let dir = temp_dir();
        let path = dir.path().join("cover.webp");

        write_artwork(&path, b"cover bytes")
            .await
            .expect("the artwork should be written");

        assert_eq!(fs::read(&path).await.unwrap(), b"cover bytes");
        assert_eq!(
            files(&dir),
            ["cover.webp"],
            "a temporary file was left over"
        );
    }

    #[tokio::test]
    async fn artwork_that_is_already_there_is_not_written_again() {
        let dir = temp_dir();
        let path = dir.path().join("cover.webp");

        write_artwork(&path, b"cover bytes").await.unwrap();
        write_artwork(&path, b"other bytes")
            .await
            .expect("writing the same artwork twice should not fail");

        assert_eq!(fs::read(&path).await.unwrap(), b"cover bytes");
        assert_eq!(
            files(&dir),
            ["cover.webp"],
            "the skipped write touched disk"
        );
    }

    #[tokio::test]
    async fn missing_directories_are_created() {
        let dir = temp_dir();
        let path = dir.path().join("ab").join("cd").join("cover.webp");

        write_artwork(&path, b"cover bytes")
            .await
            .expect("the artwork should be written");

        assert!(fs::try_exists(&path).await.unwrap());
    }

    #[tokio::test]
    async fn two_writers_of_the_same_artwork_do_not_collide() {
        let dir = temp_dir();
        let path = dir.path().join("cover.webp");

        let (first, second) = tokio::join!(
            write_artwork(&path, b"cover bytes"),
            write_artwork(&path, b"cover bytes")
        );

        first.expect("the first write should succeed");
        second.expect("the second write should succeed");
        assert_eq!(files(&dir), ["cover.webp"]);
    }

    #[tokio::test]
    async fn a_write_that_fails_leaves_nothing_behind() {
        let dir = temp_dir();
        let blocker = dir.path().join("ab");
        fs::write(&blocker, b"not a directory").await.unwrap();

        write_artwork(&blocker.join("cover.webp"), b"cover bytes")
            .await
            .expect_err("the artwork should not be written");

        assert_eq!(files(&dir), ["ab"]);
    }

    const RED: Rgb<u8> = Rgb([200, 30, 30]);
    const BLUE: Rgb<u8> = Rgb([30, 120, 200]);

    const FLUSH: &str = "flush";
    const FLUSH_GAME: u64 = 10_000;

    const READY_TIMEOUT: Duration = Duration::from_secs(30);

    fn cover(color: Rgb<u8>) -> Vec<u8> {
        let img = DynamicImage::ImageRgb8(RgbImage::from_pixel(400, 600, color));

        let mut bytes = Vec::new();
        img.write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
            .expect("the cover should encode");

        bytes
    }

    fn ok(bytes: Vec<u8>) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_bytes(bytes)
    }

    fn rate_limited(retry_after: u64) -> ResponseTemplate {
        ResponseTemplate::new(429).insert_header(RETRY_AFTER, retry_after.to_string())
    }

    fn route(name: &str) -> String {
        format!("/{name}.png")
    }

    fn mock(name: &str, response: ResponseTemplate) -> Mock {
        Mock::given(method("GET"))
            .and(path(route(name)))
            .respond_with(response)
    }

    #[derive(Debug, Clone, PartialEq)]
    struct Stored {
        game_id: GameId,
        hash: String,
        kind: ArtworkKind,
        position: u32,
        accent: Option<Color>,
    }

    #[derive(Default)]
    struct Recorder {
        stored: Mutex<Vec<Stored>>,
        refused: Option<GameId>,
    }

    impl Recorder {
        fn new() -> Arc<Self> {
            Arc::new(Self::default())
        }

        fn refusing(game_id: u64) -> Arc<Self> {
            Arc::new(Self {
                refused: Some(GameId::from(game_id)),
                ..Self::default()
            })
        }
    }

    impl ArtworkRepository for Recorder {
        fn insert(&self, game_id: GameId, artwork: &Artwork) -> anyhow::Result<()> {
            anyhow::ensure!(
                self.refused != Some(game_id),
                "the repository refused {game_id:?}"
            );

            self.stored
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(Stored {
                    game_id,
                    hash: artwork.hash.clone(),
                    kind: artwork.kind,
                    position: artwork.position,
                    accent: artwork.accent,
                });

            Ok(())
        }
    }

    fn manager(
        artwork_dir: &Path,
        max_concurrency: usize,
        repository: Arc<dyn ArtworkRepository>,
    ) -> ArtworkManager {
        let connectivity = ConnectivityManager::new(
            TokioHandle::current(),
            ConnectivityConfig {
                probe_endpoints: Vec::new(),
                ..ConnectivityConfig::default()
            },
        );

        ArtworkManager::new(
            TokioHandle::current(),
            max_concurrency,
            artwork_dir.to_path_buf(),
            connectivity,
            repository,
        )
    }

    struct Harness {
        manager: ArtworkManager,
        updates: broadcast::Receiver<ArtworkReady>,
        repository: Arc<Recorder>,
        server: MockServer,
        dir: TempDir,
        flushes: u64,
    }

    async fn harness() -> Harness {
        harness_with(Recorder::new()).await
    }

    async fn harness_with(repository: Arc<Recorder>) -> Harness {
        let dir = temp_dir();
        let server = MockServer::start().await;
        mock(FLUSH, ok(cover(BLUE))).mount(&server).await;

        let manager = manager(dir.path(), 1, repository.clone());

        Harness {
            updates: manager.subscribe(),
            manager,
            repository,
            server,
            dir,
            flushes: 0,
        }
    }

    impl Harness {
        fn url(&self, name: &str) -> String {
            format!("{}{}", self.server.uri(), route(name))
        }

        async fn serve(&self, name: &str, response: ResponseTemplate) -> String {
            mock(name, response).mount(&self.server).await;
            self.url(name)
        }

        async fn serve_once(&self, name: &str, response: ResponseTemplate) {
            mock(name, response)
                .up_to_n_times(1)
                .mount(&self.server)
                .await;
        }

        fn enqueue(&self, game_id: u64, url: &str) {
            self.manager
                .enqueue(task(game_id, 0, url))
                .expect("the task should queue");
        }

        async fn ready(&mut self) -> ArtworkReady {
            timeout(READY_TIMEOUT, self.updates.recv())
                .await
                .expect("the artwork should have been announced")
                .expect("the updates channel should stay open")
        }

        async fn flush(&mut self) {
            self.flushes += 1;
            let game_id = FLUSH_GAME + self.flushes;

            self.enqueue(game_id, &self.url(FLUSH));
            while self.ready().await.game_id != GameId::from(game_id) {}
        }

        async fn requested(&self) -> Vec<String> {
            let flushed = route(FLUSH);

            self.server
                .received_requests()
                .await
                .expect("the server should record the requests it gets")
                .iter()
                .map(|request| request.url.path().to_string())
                .filter(|path| *path != flushed)
                .collect()
        }

        fn stored(&self) -> Vec<Stored> {
            self.repository
                .stored
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
        }

        fn artwork_file(&self, hash: &str) -> PathBuf {
            artwork_path_in(self.dir.path(), hash, "webp").expect("artworh path should be valid")
        }

        fn covers_on_disk(&self) -> Vec<PathBuf> {
            files_under(self.dir.path())
        }
    }

    #[tokio::test]
    async fn a_queued_cover_is_downloaded_stored_and_announced() {
        let mut harness = harness().await;
        let url = harness.serve("cover", ok(cover(RED))).await;

        harness.enqueue(1, &url);
        let ready = harness.ready().await;

        assert_eq!(ready.game_id, GameId::from(1));
        let bytes = fs::read(harness.artwork_file(&ready.hash))
            .await
            .expect("the cover should be on disk under its hash");
        image::load_from_memory(&bytes).expect("the cover on disk should be an image");

        let accent = ready.accent.expect("a red cover should have an accent");
        assert!(
            accent.r > accent.g && accent.r > accent.b,
            "expected a red accent, got {accent:?}"
        );

        assert_eq!(
            harness.stored(),
            [Stored {
                game_id: GameId::from(1),
                hash: ready.hash,
                kind: ArtworkKind::Cover,
                position: 0,
                accent: ready.accent,
            }]
        );
    }

    #[tokio::test]
    async fn a_cover_the_server_does_not_have_is_not_asked_for_again() {
        let mut harness = harness().await;
        let url = harness.serve("missing", ResponseTemplate::new(404)).await;

        harness.enqueue(1, &url);
        harness.flush().await;

        harness.enqueue(2, &url);
        harness.flush().await;

        assert_eq!(harness.requested().await, ["/missing.png"]);
    }

    #[tokio::test]
    async fn a_rate_limited_host_is_asked_again_once_the_wait_is_over() {
        let mut harness = harness().await;
        harness.serve_once("cover", rate_limited(0)).await;
        let url = harness.serve("cover", ok(cover(RED))).await;

        harness.enqueue(1, &url);
        let ready = harness.ready().await;

        assert!(
            fs::try_exists(harness.artwork_file(&ready.hash))
                .await
                .unwrap()
        );
        assert_eq!(
            harness.requested().await,
            ["/cover.png", "/cover.png"],
            "the host should have been asked again after the wait"
        );
    }

    #[tokio::test]
    async fn a_cover_the_repository_refuses_does_not_stop_the_queue() {
        let mut harness = harness_with(Recorder::refusing(1)).await;
        let url = harness.serve("cover", ok(cover(RED))).await;

        harness.enqueue(1, &url);
        harness.flush().await;

        let stored = harness.stored();
        assert!(
            stored
                .iter()
                .all(|stored| stored.game_id != GameId::from(1)),
            "the refused artwork should not be recorded"
        );
        assert!(
            !stored.is_empty(),
            "the queue should have kept going after the refusal"
        );
    }

    #[tokio::test]
    async fn two_games_that_share_a_cover_share_one_file() {
        let mut harness = harness().await;
        let url = harness.serve("cover", ok(cover(RED))).await;

        harness.enqueue(1, &url);
        harness.enqueue(2, &url);

        let first = harness.ready().await;
        let second = harness.ready().await;

        assert_eq!(first.hash, second.hash);
        assert_eq!(harness.stored().len(), 2, "both games should be recorded");
        assert_eq!(
            harness.covers_on_disk(),
            [harness.artwork_file(&first.hash)],
            "the shared cover should only be written once"
        );
    }

    #[tokio::test]
    async fn a_server_that_keeps_failing_is_retried_and_then_set_aside() {
        if std::env::var("RUN_SLOW_TESTS").is_err() {
            return;
        }

        let mut harness = harness().await;
        let url = harness.serve("broken", ResponseTemplate::new(500)).await;

        harness.enqueue(1, &url);
        harness.flush().await;

        let attempts = harness.requested().await.len();
        assert_eq!(attempts, MAX_ATTEMPTS as usize);

        harness.enqueue(2, &url);
        harness.flush().await;

        assert_eq!(
            harness.requested().await.len(),
            attempts,
            "a url that was given up on should not be asked for again"
        );
    }
}
