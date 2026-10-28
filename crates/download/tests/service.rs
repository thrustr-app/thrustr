use artwork::ArtworkService;
use async_trait::async_trait;
use component::{ComponentRegistry, RegisterError, RegistryContext};
use domain::{
    component::{Component, Error, Metadata, Origin, Storefront},
    download::{DownloadId, DownloadPhase, DownloadReporter, NewDownload},
    game::{Game, GameId, GameRepository, GameSource, GameVersion, GameVersionDetails, NewGame},
};
use download::{DownloadService, DownloadState};
use net::{ConnectivityConfig, ConnectivityManager};
use runtime::TokioHandle;
use semver::Version;
use sqlite::SqliteStorage;
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};
use tempfile::TempDir;
use tokio::sync::{mpsc, oneshot};

const STOREFRONT: &str = "fake";
const GAMES: [&str; 3] = ["Hades", "Celeste", "Inscryption"];
const TIMEOUT: Duration = Duration::from_secs(5);

struct Call {
    game: String,
    reporter: Arc<dyn DownloadReporter>,
    result: oneshot::Sender<Result<(), Error>>,
}

impl Call {
    fn progress(&self, phase: DownloadPhase, processed: u64, total: Option<u64>) {
        self.reporter.progress(phase, processed, total);
    }

    #[track_caller]
    fn finish(self, result: Result<(), Error>) {
        assert!(
            self.result.send(result).is_ok(),
            "the download of {} should still be running",
            self.game
        );
    }

    /// Completes once the service drops the download.
    async fn stopped(mut self) {
        tokio::time::timeout(TIMEOUT, self.result.closed())
            .await
            .unwrap_or_else(|_| panic!("the download of {} should stop", self.game));
    }
}

struct FakeStorefront {
    calls: mpsc::UnboundedSender<Call>,
}

#[async_trait]
impl Storefront for FakeStorefront {
    async fn games(&self) -> Result<Vec<NewGame>, Error> {
        Ok(GAMES.map(new_game).into())
    }

    async fn game_versions(
        &self,
        _game: Game,
        _path: Vec<String>,
    ) -> Result<Vec<GameVersion>, Error> {
        Ok(vec![])
    }

    async fn game_version_details(
        &self,
        _game: Game,
        _path: Vec<String>,
    ) -> Result<GameVersionDetails, Error> {
        Ok(GameVersionDetails {
            install_size: None,
            download_size: None,
        })
    }

    async fn download(
        &self,
        game: Game,
        _path: Vec<String>,
        _target_dir: PathBuf,
        reporter: Arc<dyn DownloadReporter>,
    ) -> Result<(), Error> {
        let (result, finished) = oneshot::channel();
        let call = Call {
            game: game.name,
            reporter,
            result,
        };
        if self.calls.send(call).is_err() {
            return Err(Error::Other("the test is over".into()));
        }
        finished
            .await
            .unwrap_or_else(|_| Err(Error::Other("the test dropped the download".into())))
    }
}

struct FakeComponent {
    version: Version,
    init: Result<(), Error>,
    storefront: Arc<FakeStorefront>,
}

#[async_trait]
impl Component for FakeComponent {
    fn metadata(&self) -> Metadata<'_> {
        Metadata {
            id: STOREFRONT,
            name: "Fake",
            origin: Origin::Core,
            description: None,
            icon: None,
            version: &self.version,
            authors: &[],
        }
    }

    async fn init(&self) -> Result<(), Error> {
        self.init.clone()
    }

    fn storefront(&self) -> Option<Arc<dyn Storefront>> {
        Some(self.storefront.clone())
    }
}

fn new_game(name: &str) -> NewGame {
    NewGame {
        name: name.to_string(),
        source: GameSource {
            id: STOREFRONT.to_string(),
            lookup_id: name.to_string(),
            external_ids: HashMap::new(),
        },
        cover_url: None,
        summary: None,
        description: None,
    }
}

struct Harness {
    _dir: TempDir,
    storage: Arc<SqliteStorage>,
    registry: ComponentRegistry,
    connectivity: ConnectivityManager,
    games: HashMap<String, GameId>,
    calls_tx: mpsc::UnboundedSender<Call>,
    calls: mpsc::UnboundedReceiver<Call>,
    service: DownloadService,
}

impl Harness {
    fn unregistered() -> Self {
        let dir = TempDir::new().unwrap();
        let storage = Arc::new(SqliteStorage::new(dir.path().join("library.sqlite")).unwrap());
        let games = GAMES.map(new_game);
        storage.sync(STOREFRONT, &games).unwrap();

        let index = storage.list_index(None).unwrap();
        let games = storage
            .list_by_ids(&index.ids)
            .unwrap()
            .into_iter()
            .map(|game| (game.name, game.id))
            .collect();

        let tokio_handle = TokioHandle::current();
        let connectivity = ConnectivityManager::new(
            tokio_handle.clone(),
            ConnectivityConfig {
                probe_endpoints: vec![],
                ..ConnectivityConfig::default()
            },
        );
        let registry = ComponentRegistry::new(RegistryContext {
            tokio_handle: tokio_handle.clone(),
            component_storage: storage.clone(),
            game_repository: storage.clone(),
            artwork_service: ArtworkService::new(
                tokio_handle,
                connectivity.clone(),
                storage.clone(),
                storage.clone(),
            ),
        });

        let (calls_tx, calls) = mpsc::unbounded_channel();
        let service = start(&storage, &registry, &connectivity);

        Self {
            _dir: dir,
            storage,
            registry,
            connectivity,
            games,
            calls_tx,
            calls,
            service,
        }
    }

    async fn new() -> Self {
        let harness = Self::unregistered();
        harness.register(Ok(())).await;
        harness
    }

    async fn register(&self, init: Result<(), Error>) {
        let component = FakeComponent {
            version: Version::new(1, 0, 0),
            init,
            storefront: Arc::new(FakeStorefront {
                calls: self.calls_tx.clone(),
            }),
        };
        let handle = self
            .registry
            .register(async |_| Ok::<_, RegisterError>(component))
            .await
            .unwrap();
        let _ = handle.enable().await;
    }

    fn restart(&mut self) {
        self.service = start(&self.storage, &self.registry, &self.connectivity);
    }

    fn set_library(&self, names: &[&str]) {
        let games: Vec<NewGame> = names.iter().copied().map(new_game).collect();
        self.storage.sync(STOREFRONT, &games).unwrap();
    }

    fn try_enqueue(&self, name: &str) -> anyhow::Result<()> {
        self.service.enqueue(&new_download(self.games[name]))?;
        Ok(())
    }

    fn enqueue(&self, names: &[&str]) {
        for name in names {
            self.try_enqueue(name).unwrap();
        }
    }

    fn pause(&self) {
        self.service.set_paused(true).unwrap();
    }

    fn resume(&self) {
        self.service.set_paused(false).unwrap();
    }

    fn cancel(&self, name: &str) {
        self.service.cancel(self.id(name)).unwrap();
    }

    fn move_to(&self, name: &str, position: u32) {
        self.service.move_to(self.id(name), position).unwrap();
    }

    fn retry(&self, name: &str) {
        self.service.retry(self.id(name)).unwrap();
    }

    fn id(&self, name: &str) -> DownloadId {
        let game_id = self.games[name];
        self.service
            .state()
            .downloads()
            .iter()
            .find(|d| d.game_id == game_id)
            .unwrap_or_else(|| panic!("{name} should be queued"))
            .id
    }

    async fn call(&mut self, game: &str) -> Call {
        let call = tokio::time::timeout(TIMEOUT, self.calls.recv())
            .await
            .unwrap_or_else(|_| panic!("the download of {game} should start"))
            .unwrap();
        assert_eq!(call.game, game, "the wrong download started");
        call
    }

    /// `>` marks the running download and `!` a failed one. The running
    /// download shows its `[activity]`, and reported progress shows as
    /// `(phase processed/total)`.
    fn render(&self, state: &DownloadState) -> String {
        let running = state.running().map(|d| d.id);
        let downloads = state
            .downloads()
            .iter()
            .map(|d| {
                let name = self
                    .games
                    .iter()
                    .find_map(|(name, &id)| (id == d.game_id).then_some(name.as_str()))
                    .expect("downloads should be for known games");
                let marker = if Some(d.id) == running { ">" } else { "" };
                let failed = if d.error.is_some() { "!" } else { "" };
                let activity = match state.activity() {
                    Some(activity) if Some(d.id) == running => format!("[{activity:?}]"),
                    _ => String::new(),
                };
                let progress = match (d.phase, d.processed, d.total) {
                    (DownloadPhase::Downloading, 0, None) => String::new(),
                    (phase, processed, total) => {
                        let total = total.map_or("?".to_string(), |total| total.to_string());
                        format!("({phase:?} {processed}/{total})")
                    }
                };
                format!("{marker}{name}{failed}{activity}{progress}")
            })
            .collect::<Vec<_>>()
            .join(" ");

        match state.is_paused() {
            true => format!("paused: {downloads}"),
            false => downloads,
        }
    }

    async fn check(&self, expected: &str) {
        let mut state = self.service.subscribe();
        let _ = tokio::time::timeout(TIMEOUT, state.wait_for(|s| self.render(s) == expected)).await;
        assert_eq!(self.render(&self.service.state()), expected);
    }
}

fn auth_error() -> Error {
    Error::Auth("expired".into())
}

fn network_error() -> Error {
    Error::Other("connection reset".into())
}

fn start(
    storage: &Arc<SqliteStorage>,
    registry: &ComponentRegistry,
    connectivity: &ConnectivityManager,
) -> DownloadService {
    DownloadService::new(
        TokioHandle::current(),
        registry.clone(),
        connectivity.clone(),
        storage.clone(),
        storage.clone(),
        storage.clone(),
    )
    .unwrap()
}

fn new_download(game_id: GameId) -> NewDownload {
    NewDownload {
        game_id,
        version_path: vec!["windows".to_string()],
        target_dir: format!("/games/{game_id}").into(),
        install_size: Some(200),
        download_size: Some(100),
    }
}

#[tokio::test]
async fn queued_downloads_run_in_order() {
    let mut harness = Harness::new().await;
    harness.enqueue(&["Hades", "Celeste"]);

    let call = harness.call("Hades").await;
    harness.check(">Hades[Downloading] Celeste").await;
    call.finish(Ok(()));

    let call = harness.call("Celeste").await;
    harness.check(">Celeste[Downloading]").await;
    call.finish(Ok(()));
    harness.check("").await;
}

#[tokio::test]
async fn the_latest_progress_is_shown() {
    let mut harness = Harness::new().await;
    harness.enqueue(&["Hades"]);
    let call = harness.call("Hades").await;

    call.progress(DownloadPhase::Downloading, 40, Some(80));
    harness
        .check(">Hades[Downloading](Downloading 40/80)")
        .await;

    call.progress(DownloadPhase::Installing, 10, None);
    harness.check(">Hades[Downloading](Installing 10/?)").await;
}

#[tokio::test]
async fn pausing_and_resuming_work() {
    let mut harness = Harness::new().await;
    harness.enqueue(&["Hades"]);
    let call = harness.call("Hades").await;

    harness.pause();
    call.stopped().await;
    harness.check("paused: Hades").await;

    harness.resume();
    harness.call("Hades").await;
}

#[tokio::test]
async fn cancelling_the_running_download_starts_the_next() {
    let mut harness = Harness::new().await;
    harness.enqueue(&["Hades", "Celeste"]);
    let call = harness.call("Hades").await;

    harness.cancel("Hades");
    call.stopped().await;
    harness.call("Celeste").await;
    harness.check(">Celeste[Downloading]").await;
}

#[tokio::test]
async fn moving_another_download_first_switches_to_it() {
    let mut harness = Harness::new().await;
    harness.enqueue(&["Hades", "Celeste"]);
    let call = harness.call("Hades").await;

    harness.move_to("Celeste", 0);
    call.stopped().await;
    harness.call("Celeste").await;
    harness.check(">Celeste[Downloading] Hades").await;
}

#[tokio::test]
async fn moving_the_running_download_behind_a_failed_one_keeps_it_running() {
    let mut harness = Harness::new().await;
    harness.set_library(&["Celeste"]);
    harness.enqueue(&["Hades", "Celeste"]);
    let call = harness.call("Celeste").await;
    harness.check("Hades! >Celeste[Downloading]").await;

    harness.move_to("Celeste", 0);
    harness.check(">Celeste[Downloading] Hades!").await;
    call.finish(Ok(()));
    harness.check("Hades!").await;
}

#[tokio::test]
async fn a_game_removed_from_the_library_fails_and_the_queue_continues() {
    let mut harness = Harness::new().await;
    harness.set_library(&["Celeste"]);
    harness.enqueue(&["Hades", "Celeste"]);

    harness.call("Celeste").await;
    harness.check("Hades! >Celeste[Downloading]").await;
}

#[tokio::test]
async fn retrying_a_failed_download_starts_it_again() {
    let mut harness = Harness::new().await;
    harness.set_library(&[]);
    harness.enqueue(&["Hades"]);
    harness.check("Hades!").await;

    harness.set_library(&GAMES);
    harness.retry("Hades");
    harness.call("Hades").await;
    harness.check(">Hades[Downloading]").await;
}

#[tokio::test]
async fn an_auth_error_fails_the_download() {
    let mut harness = Harness::new().await;
    harness.enqueue(&["Hades"]);

    harness.call("Hades").await.finish(Err(auth_error()));
    harness.check("Hades!").await;
}

#[tokio::test]
async fn a_logged_out_storefront_fails_its_downloads() {
    let harness = Harness::unregistered();
    harness.register(Err(auth_error())).await;
    harness.enqueue(&["Hades"]);
    harness.check("Hades!").await;
}

#[tokio::test]
async fn a_storefront_that_is_loading_is_waited_for() {
    let mut harness = Harness::unregistered();
    harness.enqueue(&["Hades"]);
    harness.check(">Hades[WaitingForStorefront]").await;

    harness.register(Ok(())).await;
    harness.call("Hades").await;
}

#[tokio::test]
async fn a_offline_network_error_waits_for_the_connection() {
    let mut harness = Harness::new().await;
    harness.enqueue(&["Hades"]);

    harness.call("Hades").await.finish(Err(network_error()));
    harness.check(">Hades[WaitingForConnection]").await;
}

#[tokio::test]
async fn a_restarted_service_resumes_the_running_download() {
    let mut harness = Harness::new().await;
    harness.enqueue(&["Hades"]);
    let call = harness.call("Hades").await;

    harness.restart();
    call.stopped().await;
    harness.call("Hades").await;
}

#[tokio::test]
async fn the_queue_survives_a_restart() {
    let mut harness = Harness::new().await;
    harness.enqueue(&["Hades", "Celeste"]);
    let call = harness.call("Hades").await;
    call.progress(DownloadPhase::Installing, 40, Some(80));

    harness.pause();
    call.stopped().await;
    harness.restart();
    harness
        .check("paused: Hades(Installing 40/80) Celeste")
        .await;

    harness.resume();
    harness.call("Hades").await;
}

#[tokio::test]
async fn a_game_can_only_be_queued_once() {
    let mut harness = Harness::new().await;
    harness.enqueue(&["Hades"]);
    harness.call("Hades").await;

    assert!(harness.try_enqueue("Hades").is_err());
    harness.check(">Hades[Downloading]").await;
}
