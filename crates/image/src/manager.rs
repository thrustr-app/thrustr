use crate::{ImageTask, processing::process_task};
use connectivity::ConnectivityManager;
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
    task::JoinSet,
};

const MAX_ATTEMPTS: u32 = 3;
const RECOVERY_JITTER_MAX_MS: u64 = 500;

pub struct ImageManager {
    sender: mpsc::UnboundedSender<ImageTask>,
    pending: Arc<AtomicUsize>,
    active: Arc<AtomicUsize>,
    max_concurrent: Arc<AtomicUsize>,
    paused: Arc<AtomicBool>,
    wakeup: Arc<Notify>,
}

impl ImageManager {
    pub fn new(max_concurrent: usize, connectivity: ConnectivityManager) -> Self {
        let (sender, mut receiver) = mpsc::unbounded_channel::<ImageTask>();

        let pending = Arc::new(AtomicUsize::new(0));
        let active = Arc::new(AtomicUsize::new(0));
        let max_concurrent = Arc::new(AtomicUsize::new(max_concurrent));
        let paused = Arc::new(AtomicBool::new(false));
        let wakeup = Arc::new(Notify::new());
        let client = Client::new();

        tokio::spawn({
            let (pending, active, max_concurrent, paused, wakeup) = (
                pending.clone(),
                active.clone(),
                max_concurrent.clone(),
                paused.clone(),
                wakeup.clone(),
            );
            let mut connectivity_rx = connectivity.subscribe();

            async move {
                let mut join_set = JoinSet::new();

                loop {
                    while join_set.try_join_next().is_some() {
                        pending.fetch_sub(1, Ordering::Relaxed);
                        active.fetch_sub(1, Ordering::Relaxed);
                    }

                    while connectivity_rx.borrow_and_update().is_offline() {
                        tokio::select! {
                            _ = connectivity_rx.changed() => {}
                            Some(_) = join_set.join_next() => {
                                pending.fetch_sub(1, Ordering::Relaxed);
                                active.fetch_sub(1, Ordering::Relaxed);
                            }
                        }
                    }

                    if paused.load(Ordering::Acquire) {
                        tokio::select! {
                            _ = wakeup.notified() => {},
                            Some(_) = join_set.join_next() => {
                                pending.fetch_sub(1, Ordering::Relaxed);
                                active.fetch_sub(1, Ordering::Relaxed);
                            }
                        }
                        continue;
                    }

                    let at_capacity =
                        active.load(Ordering::Acquire) >= max_concurrent.load(Ordering::Acquire);

                    if at_capacity {
                        tokio::select! {
                            Some(_) = join_set.join_next() => {
                                pending.fetch_sub(1, Ordering::Relaxed);
                                active.fetch_sub(1, Ordering::Relaxed);
                            }
                            _ = wakeup.notified() => {}
                        }
                        continue;
                    }

                    tokio::select! {
                        task = receiver.recv() => match task {
                            Some(task) => {
                                join_set.spawn(run_with_retry(
                                    task,
                                    client.clone(),
                                    connectivity.clone()
                                ));
                                active.fetch_add(1, Ordering::Relaxed);
                            }
                            None => break,
                        },
                        _ = wakeup.notified() => {}
                    }
                }

                while join_set.join_next().await.is_some() {
                    pending.fetch_sub(1, Ordering::Relaxed);
                    active.fetch_sub(1, Ordering::Relaxed);
                }
            }
        });

        Self {
            sender,
            pending,
            active,
            max_concurrent,
            paused,
            wakeup,
        }
    }

    pub fn enqueue(&self, task: ImageTask) -> Result<(), mpsc::error::SendError<ImageTask>> {
        self.pending.fetch_add(1, Ordering::Relaxed);
        self.sender.send(task).inspect_err(|_| {
            self.pending.fetch_sub(1, Ordering::Relaxed);
        })
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Acquire)
    }

    pub fn pause(&self) {
        self.paused.store(true, Ordering::Release);
    }

    pub fn resume(&self) {
        self.paused.store(false, Ordering::Release);
        self.wakeup.notify_one();
    }

    pub fn max_concurrent(&self) -> usize {
        self.max_concurrent.load(Ordering::Acquire)
    }

    pub fn set_max_concurrent(&self, max: usize) {
        self.max_concurrent.store(max, Ordering::Release);
        self.wakeup.notify_one();
    }

    pub fn active(&self) -> usize {
        self.active.load(Ordering::Acquire)
    }

    pub fn pending(&self) -> usize {
        self.pending.load(Ordering::Acquire)
    }
}

async fn run_with_retry(task: ImageTask, client: Client, connectivity: ConnectivityManager) {
    let mut attempts = 0;
    loop {
        match process_task(task.clone(), client.clone()).await {
            Ok(_) => return,
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
