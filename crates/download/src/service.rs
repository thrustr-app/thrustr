use crate::{
    Activity, DownloadState, Speed,
    dirs::{self, DirLock, DirLocks},
    throughput::Meter,
};
use anyhow::{Result, anyhow};
use component::{ComponentRegistry, OperationError};
use domain::{
    download::{
        Download, DownloadId, DownloadOutcome, DownloadPhase, DownloadReporter, DownloadRepository,
        NewDownload,
    },
    game::{Game, GameId, GameRepository},
    state::AppStateRepository,
};
use event::Topic;
use net::{ConnectivityManager, backoff};
use runtime::{AbortOnDrop, TokioHandle, clock};
use serde_json::Value;
use std::{
    convert::Infallible,
    future,
    path::PathBuf,
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{sync::watch, time::Instant};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

type Progress = (DownloadPhase, u64, Option<u64>);

const PAUSED_KEY: &str = "downloads.paused";
const MAX_ATTEMPTS: u32 = 3;
const SAVE_INTERVAL: Duration = Duration::from_secs(5);
const SPEED_INTERVAL: Duration = Duration::from_secs(1);
const SPEED_WINDOW: Duration = Duration::from_secs(5);

#[derive(Clone)]
pub struct DownloadService {
    inner: Arc<Inner>,
    _supervisor: Arc<AbortOnDrop<()>>,
}

impl DownloadService {
    pub fn new(
        tokio_handle: TokioHandle,
        registry: ComponentRegistry,
        connectivity: ConnectivityManager,
        downloads: Arc<dyn DownloadRepository>,
        games: Arc<dyn GameRepository>,
        app_state: Arc<dyn AppStateRepository>,
    ) -> Result<Self> {
        let paused = app_state
            .get(PAUSED_KEY)?
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let state = DownloadState::new(downloads.list()?, paused);

        let inner = Arc::new(Inner {
            state: watch::Sender::new(state),
            writes: Mutex::new(()),
            dirs: DirLocks::default(),
            tokio_handle: tokio_handle.clone(),
            downloads,
            games,
            app_state,
            registry,
            connectivity,
        });
        let supervisor = tokio_handle.spawn(inner.clone().supervise());

        Ok(Self {
            inner,
            _supervisor: Arc::new(AbortOnDrop(supervisor)),
        })
    }

    pub fn state(&self) -> DownloadState {
        self.inner.state.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<DownloadState> {
        self.inner.state.subscribe()
    }

    pub fn enqueue(&self, download: &NewDownload) -> Result<Download> {
        let _write = self.inner.lock_writes();
        let download = self.inner.downloads.enqueue(download)?;
        self.inner.modify(|state| {
            state.push(download.clone());
            true
        });
        Ok(download)
    }

    pub fn cancel(&self, id: DownloadId) -> Result<()> {
        let dir = self
            .inner
            .state
            .borrow()
            .get(id)
            .map(|d| d.target_dir.clone());
        let delete = dir.map(|dir| {
            let lock = self.inner.dirs.lock(&dir);
            self.inner.clone().delete(dir, lock)
        });
        self.inner.remove(id)?;
        if let Some(delete) = delete {
            self.inner.tokio_handle.spawn(delete);
        }
        Ok(())
    }

    pub fn move_to(&self, id: DownloadId, position: u32) -> Result<()> {
        let _write = self.inner.lock_writes();
        if self.inner.state.borrow().get(id).is_none() {
            return Err(anyhow!("download {id} is not queued"));
        }
        self.inner.downloads.move_to(id, position)?;
        self.inner.modify(|state| state.move_to(id, position));
        Ok(())
    }

    pub fn retry(&self, id: DownloadId) -> Result<()> {
        self.inner.set_error(id, None)
    }

    pub fn set_paused(&self, paused: bool) -> Result<()> {
        let _write = self.inner.lock_writes();
        self.inner.app_state.set(PAUSED_KEY, &Value::Bool(paused))?;
        self.inner.modify(|state| {
            state.set_paused(paused);
            true
        });
        Ok(())
    }
}

struct Inner {
    state: watch::Sender<DownloadState>,
    /// Held while writing storage and the respective state change.
    writes: Mutex<()>,
    dirs: DirLocks,
    tokio_handle: TokioHandle,
    downloads: Arc<dyn DownloadRepository>,
    games: Arc<dyn GameRepository>,
    app_state: Arc<dyn AppStateRepository>,
    registry: ComponentRegistry,
    connectivity: ConnectivityManager,
}

impl Inner {
    fn modify(&self, f: impl FnOnce(&mut DownloadState) -> bool) {
        if self.state.send_if_modified(f) {
            event::emit(Topic::Downloads);
        }
    }

    fn lock_writes(&self) -> MutexGuard<'_, ()> {
        self.writes.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn set_error(&self, id: DownloadId, error: Option<String>) -> Result<()> {
        let _write = self.lock_writes();
        let Some(mut download) = self.state.borrow().get(id).cloned() else {
            return Ok(());
        };
        download.error = error.clone();
        self.downloads.save(&download)?;
        self.modify(|state| state.set_error(id, error));
        Ok(())
    }

    fn remove(&self, id: DownloadId) -> Result<()> {
        let _write = self.lock_writes();
        self.downloads.remove(id)?;
        self.modify(|state| state.remove(id).is_some());
        Ok(())
    }

    fn persist_progress(&self, id: DownloadId) -> Result<()> {
        let _write = self.lock_writes();
        let download = self.state.borrow().get(id).cloned();
        match download {
            Some(download) => self.downloads.save(&download),
            None => Ok(()),
        }
    }

    fn set_activity(&self, id: DownloadId, activity: Activity) {
        self.modify(|state| state.set_activity(id, activity));
    }

    async fn delete(self: Arc<Self>, dir: PathBuf, lock: impl Future<Output = DirLock>) {
        let _lock = lock.await;
        let path = dir.clone();
        if let Err(error) = self.blocking(move |_| Ok(dirs::remove_dir(&path)?)).await {
            error!(dir = %dir.display(), %error, "failed to delete a cancelled download");
        }
    }

    async fn blocking<T: Send + 'static>(
        self: &Arc<Self>,
        f: impl FnOnce(&Self) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let inner = self.clone();
        tokio::task::spawn_blocking(move || f(&inner)).await?
    }
}

impl Inner {
    async fn supervise(self: Arc<Self>) {
        let mut state = self.state.subscribe();
        loop {
            let running = state.borrow_and_update().running().cloned();
            if let Some(download) = running {
                self.run(download).await;
            } else if state.changed().await.is_err() {
                return;
            }
        }
    }

    async fn run(self: &Arc<Self>, download: Download) {
        let id = download.id;
        let dir = download.target_dir.clone();

        self.set_activity(id, Activity::Preparing);
        let _lock = tokio::select! {
            lock = self.dirs.lock(&dir) => lock,
            () = self.stopped(id) => {
                self.modify(DownloadState::clear_run);
                return;
            }
        };

        let reporter = Arc::new(Reporter::new(id));
        let cancel = CancellationToken::new();
        let outcome = tokio::select! {
            biased;
            outcome = self.download(download, reporter.clone(), &cancel) => outcome,
            never = self.cancel_once_stopped(id, &cancel) => match never {},
            never = self.save_periodically(id) => match never {},
            never = self.measure_speed(&reporter) => match never {},
        };
        self.modify(|state| reporter.publish(state));

        let saved = match outcome {
            Some(Ok(())) => {
                info!(%id, "download finished");
                let scratch = dir.clone();
                if let Err(error) = self
                    .blocking(move |_| Ok(dirs::remove_scratch(&scratch)?))
                    .await
                {
                    warn!(%id, dir = %dir.display(), %error, "failed to remove the scratch dir");
                }
                self.blocking(move |inner| inner.remove(id)).await
            }
            Some(Err(error)) => {
                warn!(%id, %error, "download failed");
                self.blocking(move |inner| inner.set_error(id, Some(error)))
                    .await
            }
            None => {
                info!(%id, "download stopped");
                self.blocking(move |inner| inner.persist_progress(id)).await
            }
        };
        if let Err(error) = saved {
            error!(%id, %error, "failed to save download");
        }
        self.modify(DownloadState::clear_run);
    }

    async fn stopped(&self, id: DownloadId) {
        let _ = self
            .state
            .subscribe()
            .wait_for(|s| s.running_id() != Some(id))
            .await;
    }

    async fn cancel_once_stopped(&self, id: DownloadId, cancel: &CancellationToken) -> Infallible {
        self.stopped(id).await;
        cancel.cancel();
        future::pending().await
    }

    async fn save_periodically(self: &Arc<Self>, id: DownloadId) -> Infallible {
        loop {
            clock::after(SAVE_INTERVAL).await;
            if let Err(error) = self.blocking(move |inner| inner.persist_progress(id)).await {
                error!(%id, %error, "failed to save download progress");
            }
        }
    }

    async fn measure_speed(&self, reporter: &Reporter) -> Infallible {
        let mut network = Meter::new(SPEED_WINDOW);
        let mut disk = Meter::new(SPEED_WINDOW);
        loop {
            let now = Instant::now();
            let (received, written) = reporter.totals();
            network.record(now, received);
            disk.record(now, written);
            let speed = Speed {
                network: network.rate(),
                disk: disk.rate(),
            };
            self.modify(|state| reporter.publish(state) | state.set_speed(reporter.id, speed));
            clock::after(SPEED_INTERVAL).await;
        }
    }

    /// `None` if `cancel` stopped it.
    async fn download(
        self: &Arc<Self>,
        download: Download,
        reporter: Arc<dyn DownloadReporter>,
        cancel: &CancellationToken,
    ) -> Option<Result<(), String>> {
        let Download {
            id,
            game_id,
            version_path,
            target_dir,
            ..
        } = download;
        let game = match cancel.run_until_cancelled(self.load_game(game_id)).await? {
            Ok(game) => game,
            Err(error) => return Some(Err(error)),
        };
        let mut attempts = 0;

        loop {
            let mut registered = event::listen(Topic::ComponentRegistered);
            let mut status_changed = event::listen(Topic::ComponentStateChanged);

            let Some(component) = self.registry.component(&game.source.id) else {
                self.set_activity(id, Activity::WaitingForStorefront);
                let _ = cancel.run_until_cancelled(registered.changed()).await?;
                continue;
            };
            let Some(storefront) = component.storefront() else {
                let name = component.metadata().name;
                return Some(Err(format!("{name} can't download games")));
            };

            self.set_activity(id, Activity::Downloading);

            #[expect(
                clippy::disallowed_methods,
                reason = "the queue is what runs downloads"
            )]
            let result = storefront
                .download(
                    game.id,
                    version_path.clone(),
                    target_dir.clone(),
                    reporter.clone(),
                    cancel.clone(),
                )
                .await;
            let error = match result {
                Ok(DownloadOutcome::Completed) => return Some(Ok(())),
                Ok(DownloadOutcome::Cancelled) => return None,
                Err(error) => error,
            };

            match classify(&error) {
                Failure::Wait => {
                    self.set_activity(id, Activity::WaitingForStorefront);
                    let changed = async {
                        tokio::select! {
                            _ = registered.changed() => {}
                            _ = status_changed.changed() => {}
                        }
                    };
                    cancel.run_until_cancelled(changed).await?;
                }
                Failure::Transient => {
                    cancel
                        .run_until_cancelled(self.connectivity.report_error())
                        .await?;
                    if self.connectivity.state().is_offline() {
                        self.set_activity(id, Activity::WaitingForConnection);
                        cancel
                            .run_until_cancelled(self.connectivity.wait_until_online())
                            .await?;
                        continue;
                    }

                    attempts += 1;
                    if attempts >= MAX_ATTEMPTS {
                        return Some(Err(error.to_string()));
                    }
                    warn!(%id, attempts, %error, "download failed, retrying");
                    self.set_activity(id, Activity::Retrying);
                    cancel
                        .run_until_cancelled(clock::after(backoff(attempts)))
                        .await?;
                }
                Failure::Fatal => return Some(Err(error.to_string())),
            }
        }
    }

    async fn load_game(self: &Arc<Self>, id: GameId) -> Result<Game, String> {
        self.blocking(move |inner| inner.games.get(id))
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "the game is no longer in the library".to_string())
    }
}

struct Reporter {
    id: DownloadId,
    received: AtomicU64,
    written: AtomicU64,
    progress: Mutex<Option<Progress>>,
}

impl Reporter {
    fn new(id: DownloadId) -> Self {
        Self {
            id,
            received: AtomicU64::new(0),
            written: AtomicU64::new(0),
            progress: Mutex::new(None),
        }
    }

    fn totals(&self) -> (u64, u64) {
        (
            self.received.load(Ordering::Relaxed),
            self.written.load(Ordering::Relaxed),
        )
    }

    fn publish(&self, state: &mut DownloadState) -> bool {
        let progress = self
            .progress
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        progress
            .is_some_and(|(phase, processed, total)| state.report(self.id, phase, processed, total))
    }
}

impl DownloadReporter for Reporter {
    fn progress(&self, phase: DownloadPhase, processed: u64, total: Option<u64>) {
        *self.progress.lock().unwrap_or_else(PoisonError::into_inner) =
            Some((phase, processed, total));
    }

    fn received(&self, bytes: u64) {
        self.received.fetch_add(bytes, Ordering::Relaxed);
    }

    fn written(&self, bytes: u64) {
        self.written.fetch_add(bytes, Ordering::Relaxed);
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Failure {
    Wait,
    Transient,
    Fatal,
}

fn classify(error: &OperationError) -> Failure {
    match error {
        OperationError::Rejected { rejection, .. } if rejection.is_temporary() => Failure::Wait,
        OperationError::Component(error) if !error.is_fatal() => Failure::Transient,
        _ => Failure::Fatal,
    }
}
