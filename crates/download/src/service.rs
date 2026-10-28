use crate::{Activity, DownloadState};
use anyhow::{Result, anyhow};
use component::{ComponentRegistry, OperationError};
use domain::{
    component::{InactiveReason, Rejection, Status},
    download::{
        Download, DownloadId, DownloadPhase, DownloadReporter, DownloadRepository, NewDownload,
    },
    game::{Game, GameId, GameRepository},
    state::AppStateRepository,
};
use event::Topic;
use net::{ConnectivityManager, backoff};
use runtime::{TokioHandle, clock};
use serde_json::Value;
use std::{
    convert::Infallible,
    pin::pin,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::Duration,
};
use tokio::{sync::watch, task::JoinHandle};
use tracing::{error, info, warn};

const PAUSED_KEY: &str = "downloads.paused";
const MAX_ATTEMPTS: u32 = 3;
const SAVE_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Clone)]
pub struct DownloadService {
    inner: Arc<Inner>,
    _supervisor: Arc<AbortOnDrop>,
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
        self.inner.remove(id)
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

struct AbortOnDrop(JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

struct Inner {
    state: watch::Sender<DownloadState>,
    /// Held while writing storage and the respective state change.
    writes: Mutex<()>,
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
        // Outlives the select so a stopped call is dropped after saving progress.
        let mut call = pin!(self.download(download));
        let outcome = tokio::select! {
            biased;
            result = &mut call => Some(result),
            () = self.stopped(id) => None,
            never = self.save_periodically(id) => match never {},
        };

        let saved = match outcome {
            Some(Ok(())) => {
                info!(%id, "download finished");
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
        self.modify(DownloadState::clear_activity);
    }

    async fn stopped(&self, id: DownloadId) {
        let _ = self
            .state
            .subscribe()
            .wait_for(|s| s.running_id() != Some(id))
            .await;
    }

    async fn save_periodically(self: &Arc<Self>, id: DownloadId) -> Infallible {
        loop {
            clock::after(SAVE_INTERVAL).await;
            if let Err(error) = self.blocking(move |inner| inner.persist_progress(id)).await {
                error!(%id, %error, "failed to save download progress");
            }
        }
    }

    async fn download(self: &Arc<Self>, download: Download) -> Result<(), String> {
        let Download {
            id,
            game_id,
            version_path,
            target_dir,
            ..
        } = download;
        let game = self.load_game(game_id).await?;
        let reporter: Arc<dyn DownloadReporter> = Arc::new(Reporter {
            inner: self.clone(),
            id,
        });
        let mut attempts = 0;

        loop {
            let mut registered = event::listen(Topic::ComponentRegistered);
            let mut status_changed = event::listen(Topic::ComponentStateChanged);

            let Some(component) = self.registry.component(&game.source.id) else {
                self.set_activity(id, Activity::WaitingForStorefront);
                let _ = registered.changed().await;
                continue;
            };
            let Some(storefront) = component.storefront() else {
                let name = component.metadata().name;
                return Err(format!("{name} can't download games"));
            };

            self.set_activity(id, Activity::Downloading);
            let result = storefront
                .download(
                    game.id,
                    version_path.clone(),
                    target_dir.clone(),
                    reporter.clone(),
                )
                .await;
            let Err(error) = result else {
                return Ok(());
            };

            match classify(&error) {
                Failure::Wait => {
                    self.set_activity(id, Activity::WaitingForStorefront);
                    tokio::select! {
                        _ = registered.changed() => {}
                        _ = status_changed.changed() => {}
                    }
                }
                Failure::Transient => {
                    self.connectivity.report_error().await;
                    if self.connectivity.state().is_offline() {
                        self.set_activity(id, Activity::WaitingForConnection);
                        self.connectivity.wait_until_online().await;
                        continue;
                    }

                    attempts += 1;
                    if attempts >= MAX_ATTEMPTS {
                        return Err(error.to_string());
                    }
                    warn!(%id, attempts, %error, "download failed, retrying");
                    self.set_activity(id, Activity::Retrying);
                    clock::after(backoff(attempts)).await;
                }
                Failure::Fatal => return Err(error.to_string()),
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
    inner: Arc<Inner>,
    id: DownloadId,
}

impl DownloadReporter for Reporter {
    fn progress(&self, phase: DownloadPhase, processed: u64, total: Option<u64>) {
        self.inner
            .modify(|state| state.report(self.id, phase, processed, total));
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
        OperationError::Rejected { rejection, .. } => match rejection {
            Rejection::Busy(_)
            | Rejection::Expired
            | Rejection::NotAllowed(Status::Initializing)
            | Rejection::NotAllowed(Status::Inactive(InactiveReason::Disabled)) => Failure::Wait,
            Rejection::NotAllowed(_) | Rejection::Unsupported(_) => Failure::Fatal,
        },
        OperationError::Component(error) if !error.is_fatal() => Failure::Transient,
        _ => Failure::Fatal,
    }
}
