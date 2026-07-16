use runtime::TokioHandle;
use std::{
    fmt,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    net::TcpStream,
    sync::{Semaphore, watch},
    task::JoinSet,
    time::{Instant, MissedTickBehavior},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConnectivityState {
    Online,
    Offline,
}

impl ConnectivityState {
    pub fn is_online(&self) -> bool {
        matches!(self, Self::Online)
    }

    pub fn is_offline(&self) -> bool {
        matches!(self, Self::Offline)
    }
}

impl fmt::Display for ConnectivityState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Online => write!(f, "online"),
            Self::Offline => write!(f, "offline"),
        }
    }
}

#[derive(Clone)]
pub struct ConnectivityManager {
    inner: Arc<Inner>,
}

impl ConnectivityManager {
    pub fn builder(tokio_handle: TokioHandle) -> ConnectivityManagerBuilder {
        ConnectivityManagerBuilder::new(tokio_handle)
    }

    /// Instant snapshot of the current connectivity state.
    pub fn state(&self) -> ConnectivityState {
        self.inner.state()
    }

    /// Subscribe to state changes. The watcher always holds the current
    /// value, so new subscribers never miss the state that was set before
    /// they subscribed.
    pub fn subscribe(&self) -> ConnectivityWatcher {
        ConnectivityWatcher(self.inner.subscribe())
    }

    /// Blocks until online. Resolves immediately if already online.
    pub async fn wait_until_online(&self) {
        let mut watcher = self.subscribe();
        let mut state = watcher.current();

        loop {
            if state.is_online() {
                return;
            }

            tokio::select! {
                changed = watcher.changed() => {
                    let Some(new) = changed else { return };
                    state = new;
                }
                _ = tokio::time::sleep(self.inner.min_probe_interval) => {
                    state = self.check_if_stale().await;
                }
            }
        }
    }

    /// Reports an error, triggering a probe if the interval guard is not active.
    pub fn report_error(&self) {
        let permit = match Arc::clone(&self.inner.probe_sem).try_acquire_owned() {
            Ok(p) => p,
            Err(_) => return,
        };

        let inner = Arc::clone(&self.inner);
        self.inner.tokio_handle.spawn(async move {
            let _permit = permit;
            inner.run_probe_if_stale().await;
        });
    }

    /// Runs a probe right now, waiting for active probes to complete first and
    /// ignoring the interval guard.
    /// Unless the result is needed immediately, prefer [`report_error`] instead.
    pub async fn check(&self) -> ConnectivityState {
        let _permit = Arc::clone(&self.inner.probe_sem)
            .acquire_owned()
            .await
            .expect("probe semaphore closed");

        self.inner.run_probe().await
    }

    /// Runs a probe unless one finished within the interval guard, in which
    /// case the current state is returned untouched.
    async fn check_if_stale(&self) -> ConnectivityState {
        let _permit = Arc::clone(&self.inner.probe_sem)
            .acquire_owned()
            .await
            .expect("probe semaphore closed");

        self.inner.run_probe_if_stale().await
    }
}

pub struct ConnectivityWatcher(watch::Receiver<ConnectivityState>);

impl ConnectivityWatcher {
    pub fn current(&mut self) -> ConnectivityState {
        *self.0.borrow_and_update()
    }

    pub async fn changed(&mut self) -> Option<ConnectivityState> {
        self.0.changed().await.ok()?;
        Some(*self.0.borrow_and_update())
    }
}

pub struct ConnectivityManagerBuilder {
    tokio_handle: TokioHandle,
    min_probe_interval: Duration,
    poll_interval: Duration,
    probe_endpoints: Vec<String>,
    probe_timeout: Duration,
    initial_state: ConnectivityState,
}

impl ConnectivityManagerBuilder {
    pub fn new(tokio_handle: TokioHandle) -> Self {
        Self {
            tokio_handle,
            min_probe_interval: Duration::from_secs(5),
            poll_interval: Duration::from_secs(15),
            probe_endpoints: ["1.1.1.1:53", "9.9.9.9:53", "8.8.8.8:53"]
                .map(String::from)
                .into(),
            probe_timeout: Duration::from_secs(3),
            initial_state: ConnectivityState::Online,
        }
    }

    pub fn min_probe_interval(mut self, duration: Duration) -> Self {
        self.min_probe_interval = duration;
        self
    }

    pub fn poll_interval(mut self, duration: Duration) -> Self {
        self.poll_interval = duration;
        self
    }

    pub fn probe_endpoints<I>(mut self, endpoints: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<String>,
    {
        self.probe_endpoints = endpoints.into_iter().map(Into::into).collect();
        self
    }

    pub fn probe_timeout(mut self, duration: Duration) -> Self {
        self.probe_timeout = duration;
        self
    }

    pub fn initial_state(mut self, state: ConnectivityState) -> Self {
        self.initial_state = state;
        self
    }

    pub fn build(self) -> ConnectivityManager {
        let (tx, _rx) = watch::channel(self.initial_state);

        ConnectivityManager {
            inner: Arc::new(Inner {
                tokio_handle: self.tokio_handle,
                tx,
                probe_sem: Arc::new(Semaphore::new(1)),
                last_probe: Mutex::new(None),
                min_probe_interval: self.min_probe_interval,
                probe_endpoints: self.probe_endpoints,
                probe_timeout: self.probe_timeout,
            }),
        }
    }

    /// Builds the manager and spawns a background poller that probes
    /// immediately and then on every `poll_interval` tick, keeping the state
    /// fresh even when no consumer reports errors. Never blocks the caller.
    /// The initial state (optimistically `Online` by default) holds until
    /// the first probe completes.
    ///
    /// The poller holds a weak reference and stops once every manager clone
    /// has been dropped.
    pub fn build_probing(self) -> ConnectivityManager {
        let poll_interval = self.poll_interval;
        let manager = self.build();

        let weak = Arc::downgrade(&manager.inner);
        manager.inner.tokio_handle.spawn(async move {
            let mut ticker = tokio::time::interval(poll_interval);
            ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

            loop {
                ticker.tick().await;

                let Some(inner) = weak.upgrade() else { return };
                let _permit = inner
                    .probe_sem
                    .acquire()
                    .await
                    .expect("probe semaphore closed");
                inner.run_probe_if_stale().await;
            }
        });

        manager
    }
}

struct Inner {
    tokio_handle: TokioHandle,
    tx: watch::Sender<ConnectivityState>,
    probe_sem: Arc<Semaphore>,
    last_probe: Mutex<Option<Instant>>,
    min_probe_interval: Duration,
    probe_endpoints: Vec<String>,
    probe_timeout: Duration,
}

impl Inner {
    fn state(&self) -> ConnectivityState {
        *self.tx.borrow()
    }

    fn subscribe(&self) -> watch::Receiver<ConnectivityState> {
        self.tx.subscribe()
    }

    fn apply(&self, new: ConnectivityState) {
        self.tx.send_if_modified(|cur| {
            if *cur == new {
                return false;
            }
            *cur = new;
            true
        });
    }

    fn within_interval(&self) -> bool {
        self.last_probe
            .lock()
            .unwrap()
            .is_some_and(|t| t.elapsed() < self.min_probe_interval)
    }

    async fn run_probe(&self) -> ConnectivityState {
        let state = probe_all(&self.probe_endpoints, self.probe_timeout).await;
        self.apply(state);
        *self.last_probe.lock().unwrap() = Some(Instant::now());
        state
    }

    async fn run_probe_if_stale(&self) -> ConnectivityState {
        if self.within_interval() {
            return self.state();
        }

        self.run_probe().await
    }
}

async fn probe_all(endpoints: &[String], timeout: Duration) -> ConnectivityState {
    let mut set = JoinSet::new();

    for endpoint in endpoints {
        let endpoint = endpoint.clone();
        set.spawn(async move {
            tokio::time::timeout(timeout, TcpStream::connect(&endpoint))
                .await
                .is_ok_and(|r| r.is_ok())
        });
    }

    while let Some(res) = set.join_next().await {
        if res.unwrap_or(false) {
            set.abort_all();
            return ConnectivityState::Online;
        }
    }

    ConnectivityState::Offline
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        time::Duration,
    };

    async fn counting_listener() -> (String, Arc<AtomicUsize>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = listener.local_addr().unwrap().to_string();

        let probes = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&probes);
        tokio::spawn(async move {
            loop {
                let _ = listener.accept().await;
                counter.fetch_add(1, Ordering::SeqCst);
            }
        });

        (endpoint, probes)
    }

    fn offline_manager() -> ConnectivityManager {
        ConnectivityManager::builder(TokioHandle::current())
            .initial_state(ConnectivityState::Offline)
            .probe_endpoints(vec!["127.0.0.1:1"])
            .probe_timeout(Duration::from_millis(100))
            .build()
    }

    #[tokio::test]
    async fn starts_with_given_state() {
        let m = ConnectivityManager::builder(TokioHandle::current())
            .initial_state(ConnectivityState::Offline)
            .build();
        assert!(m.state().is_offline());
    }

    #[tokio::test]
    async fn wait_until_online_returns_immediately_when_online() {
        let m = ConnectivityManager::builder(TokioHandle::current())
            .initial_state(ConnectivityState::Online)
            .build();

        tokio::time::timeout(Duration::from_millis(50), m.wait_until_online())
            .await
            .expect("should return immediately when already online");
    }

    #[tokio::test]
    async fn subscriber_sees_state_change() {
        let m = offline_manager();
        let mut watcher = m.subscribe();

        assert!(watcher.current().is_offline());

        let m2 = m.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            m2.inner.apply(ConnectivityState::Online);
        });

        assert_eq!(watcher.changed().await, Some(ConnectivityState::Online));
    }

    #[tokio::test]
    async fn watcher_changed_skips_values_seen_via_current() {
        let m = offline_manager();
        let mut watcher = m.subscribe();

        m.inner.apply(ConnectivityState::Online);
        assert!(watcher.current().is_online());

        tokio::time::timeout(Duration::from_millis(50), watcher.changed())
            .await
            .expect_err("changed should wait for a newer value");
    }

    #[tokio::test]
    async fn watcher_changed_returns_none_after_manager_dropped() {
        let m = offline_manager();
        let mut watcher = m.subscribe();

        drop(m);

        assert_eq!(watcher.changed().await, None);
    }

    #[tokio::test]
    async fn build_probing_starts_optimistic_and_corrects_in_background() {
        let m = ConnectivityManager::builder(TokioHandle::current())
            .probe_endpoints(vec!["127.0.0.1:1"])
            .probe_timeout(Duration::from_millis(100))
            .build_probing();

        assert!(m.state().is_online());

        let mut watcher = m.subscribe();
        let state = tokio::time::timeout(Duration::from_secs(2), watcher.changed())
            .await
            .expect("background probe should update the state");
        assert_eq!(state, Some(ConnectivityState::Offline));
    }

    #[tokio::test]
    async fn concurrent_waiters_share_single_probe() {
        let (endpoint, probes) = counting_listener().await;

        let m = ConnectivityManager::builder(TokioHandle::current())
            .initial_state(ConnectivityState::Offline)
            .probe_endpoints(vec![endpoint])
            .min_probe_interval(Duration::from_millis(100))
            .build();

        let mut waiters = JoinSet::new();
        for _ in 0..50 {
            let m = m.clone();
            waiters.spawn(async move { m.wait_until_online().await });
        }

        tokio::time::timeout(Duration::from_secs(5), async {
            while waiters.join_next().await.is_some() {}
        })
        .await
        .expect("waiters should resolve once a probe succeeds");

        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(probes.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn report_error_deduplicated_within_interval() {
        let m = ConnectivityManager::builder(TokioHandle::current())
            .initial_state(ConnectivityState::Online)
            .min_probe_interval(Duration::from_secs(60))
            .probe_endpoints(vec!["127.0.0.1:1"])
            .probe_timeout(Duration::from_millis(50))
            .build();

        *m.inner.last_probe.lock().unwrap() = Some(Instant::now());

        m.report_error();
        tokio::time::sleep(Duration::from_millis(100)).await;

        assert!(m.state().is_online());
    }

    #[tokio::test]
    async fn report_error_probes_when_stale() {
        let m = ConnectivityManager::builder(TokioHandle::current())
            .initial_state(ConnectivityState::Online)
            .probe_endpoints(vec!["127.0.0.1:1"])
            .probe_timeout(Duration::from_millis(50))
            .build();

        let mut watcher = m.subscribe();
        m.report_error();

        let state = tokio::time::timeout(Duration::from_secs(2), watcher.changed())
            .await
            .expect("probe should flip the state");
        assert_eq!(state, Some(ConnectivityState::Offline));
    }

    #[tokio::test]
    async fn check_ignores_interval_guard() {
        let m = ConnectivityManager::builder(TokioHandle::current())
            .initial_state(ConnectivityState::Online)
            .min_probe_interval(Duration::from_secs(60))
            .probe_endpoints(vec!["127.0.0.1:1"])
            .probe_timeout(Duration::from_millis(50))
            .build();

        *m.inner.last_probe.lock().unwrap() = Some(Instant::now());

        assert!(m.check().await.is_offline());
    }

    #[tokio::test]
    async fn waiter_wakes_on_state_change() {
        let m = offline_manager();

        let waiter = tokio::spawn({
            let m = m.clone();
            async move { m.wait_until_online().await }
        });

        tokio::time::sleep(Duration::from_millis(20)).await;
        m.inner.apply(ConnectivityState::Online);

        tokio::time::timeout(Duration::from_millis(200), waiter)
            .await
            .expect("waiter should wake on state change")
            .unwrap();
    }

    #[tokio::test]
    async fn build_probing_polls_periodically() {
        let (endpoint, probes) = counting_listener().await;

        let _m = ConnectivityManager::builder(TokioHandle::current())
            .probe_endpoints(vec![endpoint])
            .min_probe_interval(Duration::from_millis(10))
            .poll_interval(Duration::from_millis(50))
            .build_probing();

        tokio::time::timeout(Duration::from_secs(2), async {
            while probes.load(Ordering::SeqCst) < 3 {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("poller should keep probing on its interval");
    }

    #[tokio::test]
    async fn poller_stops_when_manager_dropped() {
        let (endpoint, probes) = counting_listener().await;

        let m = ConnectivityManager::builder(TokioHandle::current())
            .probe_endpoints(vec![endpoint])
            .min_probe_interval(Duration::from_millis(10))
            .poll_interval(Duration::from_millis(25))
            .build_probing();

        tokio::time::timeout(Duration::from_secs(2), async {
            while probes.load(Ordering::SeqCst) == 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("poller should probe at least once");

        drop(m);
        tokio::time::sleep(Duration::from_millis(50)).await;

        let after_drop = probes.load(Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(probes.load(Ordering::SeqCst), after_drop);
    }

    #[tokio::test]
    async fn probe_online_when_any_endpoint_reachable() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        tokio::spawn(async move {
            loop {
                let _ = listener.accept().await;
            }
        });

        let m = ConnectivityManager::builder(TokioHandle::current())
            .initial_state(ConnectivityState::Offline)
            .probe_endpoints(vec!["127.0.0.1:1".to_string(), addr])
            .probe_timeout(Duration::from_millis(200))
            .build();

        assert!(m.check().await.is_online());
    }
}
