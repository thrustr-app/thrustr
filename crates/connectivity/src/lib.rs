use std::{fmt, sync::Arc, time::Duration};
use tokio::{
    net::TcpStream,
    sync::{Mutex, Semaphore, watch},
    task::{JoinHandle, JoinSet},
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

struct Inner {
    tx: watch::Sender<ConnectivityState>,
    probe_sem: Arc<Semaphore>,
    last_probe: Mutex<Option<Instant>>,
    min_probe_interval: Duration,
    probe_endpoints: Vec<&'static str>,
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

    async fn within_interval(&self) -> bool {
        self.last_probe
            .lock()
            .await
            .map_or(false, |t| t.elapsed() < self.min_probe_interval)
    }

    async fn run_probe(&self) -> ConnectivityState {
        let state = probe_all(&self.probe_endpoints, self.probe_timeout).await;
        self.apply(state);
        *self.last_probe.lock().await = Some(Instant::now());
        state
    }
}

#[derive(Clone)]
pub struct ConnectivityManager {
    inner: Arc<Inner>,
}

impl ConnectivityManager {
    pub fn new(min_probe_interval: Duration) -> Self {
        Self::builder()
            .min_probe_interval(min_probe_interval)
            .build()
    }

    pub fn builder() -> ConnectivityManagerBuilder {
        ConnectivityManagerBuilder::default()
    }

    /// Instant snapshot of the current connectivity state.
    pub fn state(&self) -> ConnectivityState {
        self.inner.state()
    }

    /// Subscribe to state changes. The receiver always holds the current value,
    /// so new subscribers never miss the state that was set before they subscribed.
    pub fn subscribe(&self) -> watch::Receiver<ConnectivityState> {
        self.inner.subscribe()
    }

    /// Blocks until online. Resolves immediately if already online.
    pub async fn wait_until_online(&self) {
        let mut rx = self.subscribe();

        loop {
            if rx.borrow().is_online() {
                return;
            }

            tokio::select! {
                res = rx.changed() => {
                    if res.is_err() { return; }
                }
                _ = tokio::time::sleep(self.inner.min_probe_interval) => {
                    self.check().await;
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
        tokio::spawn(async move {
            let _permit = permit;

            if inner.within_interval().await {
                return;
            }

            inner.run_probe().await;
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

    /// Spawns a task that probes on a regular interval.
    /// Dropping the handle will cancel the task.
    pub fn start_polling(&self, interval: Duration) -> PollerHandle {
        let manager = self.clone();

        let handle = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

            loop {
                ticker.tick().await;
                manager.check().await;
            }
        });

        PollerHandle(handle)
    }
}

pub struct PollerHandle(JoinHandle<()>);

impl Drop for PollerHandle {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub struct ConnectivityManagerBuilder {
    min_probe_interval: Duration,
    probe_endpoints: Vec<&'static str>,
    probe_timeout: Duration,
    initial_state: ConnectivityState,
}

impl Default for ConnectivityManagerBuilder {
    fn default() -> Self {
        Self {
            min_probe_interval: Duration::from_secs(5),
            probe_endpoints: vec!["1.1.1.1:53", "9.9.9.9:53", "8.8.8.8:53"],
            probe_timeout: Duration::from_secs(3),
            initial_state: ConnectivityState::Online,
        }
    }
}

impl ConnectivityManagerBuilder {
    pub fn min_probe_interval(mut self, duration: Duration) -> Self {
        self.min_probe_interval = duration;
        self
    }

    pub fn probe_endpoints(mut self, endpoints: Vec<&'static str>) -> Self {
        self.probe_endpoints = endpoints;
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
                tx,
                probe_sem: Arc::new(Semaphore::new(1)),
                last_probe: Mutex::new(None),
                min_probe_interval: self.min_probe_interval,
                probe_endpoints: self.probe_endpoints,
                probe_timeout: self.probe_timeout,
            }),
        }
    }

    pub async fn build_probing(self) -> ConnectivityManager {
        let state = probe_all(&self.probe_endpoints, self.probe_timeout).await;
        self.initial_state(state).build()
    }
}

async fn probe_all(endpoints: &[&'static str], timeout: Duration) -> ConnectivityState {
    let mut set = JoinSet::new();

    for &endpoint in endpoints {
        set.spawn(async move {
            tokio::time::timeout(timeout, TcpStream::connect(endpoint))
                .await
                .map_or(false, |r| r.is_ok())
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
    use std::time::Duration;

    fn offline_manager() -> ConnectivityManager {
        ConnectivityManagerBuilder::default()
            .initial_state(ConnectivityState::Offline)
            .probe_endpoints(vec!["127.0.0.1:1"])
            .probe_timeout(Duration::from_millis(100))
            .build()
    }

    #[tokio::test]
    async fn starts_with_given_state() {
        let m = ConnectivityManagerBuilder::default()
            .initial_state(ConnectivityState::Offline)
            .build();
        assert!(m.state().is_offline());
    }

    #[tokio::test]
    async fn wait_until_online_returns_immediately_when_online() {
        let m = ConnectivityManagerBuilder::default()
            .initial_state(ConnectivityState::Online)
            .build();

        tokio::time::timeout(Duration::from_millis(50), m.wait_until_online())
            .await
            .expect("should return immediately when already online");
    }

    #[tokio::test]
    async fn subscriber_sees_state_change() {
        let m = offline_manager();
        let mut rx = m.subscribe();

        assert!(rx.borrow().is_offline());

        let m2 = m.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            m2.inner.apply(ConnectivityState::Online);
        });

        rx.changed().await.unwrap();
        assert!(rx.borrow().is_online());
    }

    #[tokio::test]
    async fn report_error_deduplicated_within_interval() {
        let m = ConnectivityManagerBuilder::default()
            .initial_state(ConnectivityState::Online)
            .min_probe_interval(Duration::from_secs(60))
            .probe_endpoints(vec!["127.0.0.1:1"])
            .probe_timeout(Duration::from_millis(50))
            .build();

        *m.inner.last_probe.lock().await = Some(Instant::now());

        m.report_error();
        tokio::time::sleep(Duration::from_millis(100)).await;

        assert!(m.state().is_online());
    }
}
