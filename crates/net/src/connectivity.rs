use futures::{StreamExt, stream::FuturesUnordered};
use runtime::TokioHandle;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    net::TcpStream,
    sync::{Mutex as AsyncMutex, watch},
    task::JoinHandle,
    time::{Instant, MissedTickBehavior},
};
use tracing::{debug, info, warn};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConnectivityState {
    Online,
    Offline,
}

impl ConnectivityState {
    pub fn is_online(self) -> bool {
        matches!(self, Self::Online)
    }

    pub fn is_offline(self) -> bool {
        matches!(self, Self::Offline)
    }
}

#[derive(Debug, Clone)]
pub struct ConnectivityConfig {
    pub min_probe_interval: Duration,
    /// Only read by [`ConnectivityManager::spawn_probing`].
    pub poll_interval: Duration,
    pub probe_endpoints: Vec<String>,
    pub probe_timeout: Duration,
    pub initial_state: ConnectivityState,
}

impl Default for ConnectivityConfig {
    fn default() -> Self {
        Self {
            min_probe_interval: Duration::from_secs(5),
            poll_interval: Duration::from_secs(15),
            probe_endpoints: ["1.1.1.1:53", "9.9.9.9:53", "8.8.8.8:53"]
                .into_iter()
                .map(String::from)
                .collect(),
            probe_timeout: Duration::from_secs(3),
            initial_state: ConnectivityState::Online,
        }
    }
}

#[derive(Clone)]
pub struct ConnectivityManager {
    inner: Arc<Inner>,
}

impl ConnectivityManager {
    pub fn new(tokio_handle: TokioHandle, config: ConnectivityConfig) -> Self {
        if config.probe_endpoints.is_empty() {
            warn!("no probe endpoints configured, every probe will report offline");
        }

        let (tx, _rx) = watch::channel(config.initial_state);

        Self {
            inner: Arc::new(Inner {
                tokio_handle,
                tx,
                probe_slot: AsyncMutex::new(()),
                last_probe: Mutex::new(None),
                min_probe_interval: config.min_probe_interval,
                probe_endpoints: config.probe_endpoints,
                probe_timeout: config.probe_timeout,
            }),
        }
    }

    pub fn spawn_probing(tokio_handle: TokioHandle, config: ConnectivityConfig) -> Self {
        let poll_interval = config.poll_interval;
        let manager = Self::new(tokio_handle, config);

        let weak = Arc::downgrade(&manager.inner);
        manager.inner.tokio_handle.spawn(async move {
            let mut ticker = tokio::time::interval(poll_interval);
            ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

            loop {
                ticker.tick().await;

                let Some(inner) = weak.upgrade() else {
                    debug!("connectivity poller stopped");
                    return;
                };
                inner.probe_if_stale().await;
            }
        });

        manager
    }

    pub fn state(&self) -> ConnectivityState {
        self.inner.state()
    }

    pub fn subscribe(&self) -> ConnectivityWatcher {
        ConnectivityWatcher(self.inner.tx.subscribe())
    }

    /// Resolves immediately if already online. While waiting, re-probes once
    /// the `min_probe_interval` guard expires. Also resolves if the Tokio
    /// runtime shuts down.
    pub async fn wait_until_online(&self) {
        let mut watcher = self.subscribe();
        let mut state = watcher.current();

        loop {
            if state.is_online() {
                return;
            }

            tokio::select! {
                changed = watcher.changed() => {
                    state = changed.expect("sender should outlive the borrow of self");
                }
                probed = self.probe_after_interval() => {
                    // Must return, not continue, otherwise after runtime shutdown
                    // the spawn resolves instantly and a retry loop would spin.
                    let Some(new) = probed else { return };
                    state = new;
                }
            }
        }
    }

    /// Awaiting this guarantees the state has been re-checked, so a following
    /// [`Self::wait_until_online`] never acts on a pre-error reading. Skipped
    /// if a probe already ran within `min_probe_interval`.
    pub async fn report_error(&self) {
        let _ = self.spawn_probe(Duration::ZERO).await;
    }

    async fn probe_after_interval(&self) -> Option<ConnectivityState> {
        self.spawn_probe(self.inner.probe_guard_remaining())
            .await
            .ok()
    }

    fn spawn_probe(&self, delay: Duration) -> JoinHandle<ConnectivityState> {
        // Probing touches the Tokio reactor and timer, so it must run on the
        // handle rather than the caller's executor.
        let inner = Arc::clone(&self.inner);
        self.inner.tokio_handle.spawn(async move {
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            inner.probe_if_stale().await
        })
    }
}

pub struct ConnectivityWatcher(watch::Receiver<ConnectivityState>);

impl ConnectivityWatcher {
    pub fn current(&mut self) -> ConnectivityState {
        *self.0.borrow_and_update()
    }

    /// `None` once the manager is dropped and no further changes can arrive.
    pub async fn changed(&mut self) -> Option<ConnectivityState> {
        self.0.changed().await.ok()?;
        Some(*self.0.borrow_and_update())
    }
}

struct Inner {
    tokio_handle: TokioHandle,
    tx: watch::Sender<ConnectivityState>,
    probe_slot: AsyncMutex<()>,
    last_probe: Mutex<Option<Instant>>,
    min_probe_interval: Duration,
    probe_endpoints: Vec<String>,
    probe_timeout: Duration,
}

impl Inner {
    fn state(&self) -> ConnectivityState {
        *self.tx.borrow()
    }

    fn set_state(&self, new: ConnectivityState) {
        let changed = self
            .tx
            .send_if_modified(|cur| std::mem::replace(cur, new) != new);

        if changed {
            match new {
                ConnectivityState::Online => info!("connectivity restored"),
                ConnectivityState::Offline => warn!("connectivity lost"),
            }
        }
    }

    fn probe_guard_remaining(&self) -> Duration {
        self.last_probe
            .lock()
            .unwrap()
            .and_then(|t| self.min_probe_interval.checked_sub(t.elapsed()))
            .unwrap_or(Duration::ZERO)
    }

    async fn probe_if_stale(&self) -> ConnectivityState {
        let _guard = self.probe_slot.lock().await;

        if !self.probe_guard_remaining().is_zero() {
            return self.state();
        }

        let state = probe_any(&self.probe_endpoints, self.probe_timeout).await;
        // Update the timestamp before notifying. A waiter woken by the change
        // could see an expired guard and probe again immediately.
        *self.last_probe.lock().unwrap() = Some(Instant::now());
        self.set_state(state);
        state
    }
}

async fn probe_any(endpoints: &[String], timeout: Duration) -> ConnectivityState {
    let mut probes: FuturesUnordered<_> = endpoints
        .iter()
        .map(|endpoint| async move {
            tokio::time::timeout(timeout, TcpStream::connect(endpoint))
                .await
                .is_ok_and(|r| r.is_ok())
        })
        .collect();

    while let Some(reachable) = probes.next().await {
        if reachable {
            return ConnectivityState::Online;
        }
    }

    ConnectivityState::Offline
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::task::JoinSet;

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

    fn dead_endpoints() -> Vec<String> {
        vec!["127.0.0.1:1".to_owned()]
    }

    fn manager(config: ConnectivityConfig) -> ConnectivityManager {
        ConnectivityManager::new(TokioHandle::current(), config)
    }

    fn offline_manager() -> ConnectivityManager {
        manager(ConnectivityConfig {
            initial_state: ConnectivityState::Offline,
            probe_endpoints: dead_endpoints(),
            probe_timeout: Duration::from_millis(100),
            ..Default::default()
        })
    }

    #[tokio::test]
    async fn wait_until_online_returns_immediately_when_online() {
        let m = manager(ConnectivityConfig {
            initial_state: ConnectivityState::Online,
            ..Default::default()
        });

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
            m2.inner.set_state(ConnectivityState::Online);
        });

        assert_eq!(watcher.changed().await, Some(ConnectivityState::Online));
    }

    #[tokio::test]
    async fn watcher_changed_skips_values_seen_via_current() {
        let m = offline_manager();
        let mut watcher = m.subscribe();

        m.inner.set_state(ConnectivityState::Online);
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
    async fn spawn_probing_corrects_optimistic_start() {
        let m = ConnectivityManager::spawn_probing(
            TokioHandle::current(),
            ConnectivityConfig {
                probe_endpoints: dead_endpoints(),
                probe_timeout: Duration::from_millis(100),
                ..Default::default()
            },
        );

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

        let m = manager(ConnectivityConfig {
            initial_state: ConnectivityState::Offline,
            probe_endpoints: vec![endpoint],
            min_probe_interval: Duration::from_secs(1),
            ..Default::default()
        });

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
    async fn report_error_skipped_within_interval() {
        let m = manager(ConnectivityConfig {
            initial_state: ConnectivityState::Online,
            min_probe_interval: Duration::from_secs(60),
            probe_endpoints: dead_endpoints(),
            probe_timeout: Duration::from_millis(50),
            ..Default::default()
        });

        *m.inner.last_probe.lock().unwrap() = Some(Instant::now());

        m.report_error().await;

        assert!(m.state().is_online());
    }

    #[tokio::test]
    async fn report_error_probes_when_stale() {
        let m = manager(ConnectivityConfig {
            initial_state: ConnectivityState::Online,
            probe_endpoints: dead_endpoints(),
            probe_timeout: Duration::from_millis(50),
            ..Default::default()
        });

        m.report_error().await;

        assert!(m.state().is_offline());
    }

    #[tokio::test]
    async fn waiter_wakes_on_state_change() {
        let m = offline_manager();

        let waiter = tokio::spawn({
            let m = m.clone();
            async move { m.wait_until_online().await }
        });

        tokio::time::sleep(Duration::from_millis(20)).await;
        m.inner.set_state(ConnectivityState::Online);

        tokio::time::timeout(Duration::from_millis(200), waiter)
            .await
            .expect("waiter should wake on state change")
            .unwrap();
    }

    #[tokio::test]
    async fn spawn_probing_polls_periodically() {
        let (endpoint, probes) = counting_listener().await;

        let _m = ConnectivityManager::spawn_probing(
            TokioHandle::current(),
            ConnectivityConfig {
                probe_endpoints: vec![endpoint],
                min_probe_interval: Duration::from_millis(10),
                poll_interval: Duration::from_millis(50),
                ..Default::default()
            },
        );

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

        let m = ConnectivityManager::spawn_probing(
            TokioHandle::current(),
            ConnectivityConfig {
                probe_endpoints: vec![endpoint],
                min_probe_interval: Duration::from_millis(10),
                poll_interval: Duration::from_millis(25),
                ..Default::default()
            },
        );

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
    async fn probe_any_succeeds_when_one_endpoint_is_reachable() {
        let (endpoint, _probes) = counting_listener().await;
        let endpoints = vec!["127.0.0.1:1".to_owned(), endpoint];

        let state = probe_any(&endpoints, Duration::from_millis(100)).await;

        assert!(state.is_online());
    }
}
