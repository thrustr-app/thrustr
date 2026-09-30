use crate::{
    ComponentHandle, ComponentLink,
    handles::{Permit, WeakComponentHandle},
};
use domain::component::{
    Activation, PeriodicTask, Schedule, ScheduleError, Scheduler, SchedulerOperation,
};
use runtime::TokioHandle;
use std::{
    collections::HashMap,
    mem,
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};
use thiserror::Error;
use tokio::{
    select,
    sync::oneshot::{self, error::TryRecvError},
    time::sleep,
};
use tracing::{debug, warn};

const MAX_TASKS: usize = 64;
const MAX_ARGS_SIZE: usize = 64 * 1024;
const MIN_DELAY: Duration = Duration::from_secs(1);

pub struct Timers<'a>(&'a ComponentLink);

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TimersError {
    #[error("the component is not registered")]
    NotRegistered,

    #[error(transparent)]
    Schedule(#[from] ScheduleError),
}

impl<'a> Timers<'a> {
    pub(crate) fn new(link: &'a ComponentLink) -> Self {
        Self(link)
    }

    pub fn schedule(
        &self,
        task: String,
        args: Vec<u8>,
        schedule: Schedule,
    ) -> Result<(), TimersError> {
        let handle = self.0.handle().ok_or(TimersError::NotRegistered)?;
        let timers = handle.timers().ok_or(ScheduleError::Unsupported)?;
        Ok(timers.schedule(&handle, task, args, schedule)?)
    }

    pub fn cancel(&self, task: &str) {
        if let Some(handle) = self.0.handle()
            && let Some(timers) = handle.timers()
        {
            timers.cancel(task);
        }
    }
}

/// A task to run on a schedule within an activation.
struct Job {
    task: String,
    args: Vec<u8>,
    schedule: Schedule,
    activation: Activation,
}

impl Job {
    fn new(task: String, args: Vec<u8>, schedule: Schedule, activation: Activation) -> Self {
        let schedule = Schedule {
            delay: schedule.delay.max(MIN_DELAY),
            interval: schedule.interval.map(|interval| interval.max(MIN_DELAY)),
        };
        Self {
            task,
            args,
            schedule,
            activation,
        }
    }

    fn periodic(PeriodicTask { task, interval }: &PeriodicTask, activation: Activation) -> Self {
        let schedule = Schedule {
            delay: *interval,
            interval: Some(*interval),
        };
        Self::new(task.clone(), Vec::new(), schedule, activation)
    }
}

type Pending = HashMap<String, Entry>;

enum Entry {
    /// Scheduled while initializing. Its timer starts once the component is
    /// active.
    Waiting(Job),
    /// Its timer is running until the entry is dropped.
    Started {
        activation: Activation,
        _cancel: oneshot::Sender<()>,
    },
}

impl Entry {
    fn activation(&self) -> Activation {
        match self {
            Self::Waiting(job) => job.activation,
            Self::Started { activation, .. } => *activation,
        }
    }
}

pub(crate) struct ComponentTimers {
    scheduler: Arc<dyn Scheduler>,
    tokio_handle: TokioHandle,
    pending: Mutex<Pending>,
}

impl ComponentTimers {
    pub(crate) fn new(scheduler: Arc<dyn Scheduler>, tokio_handle: TokioHandle) -> Self {
        Self {
            scheduler,
            tokio_handle,
            pending: Mutex::default(),
        }
    }

    /// Starts the timers of tasks scheduled while initializing and periodic tasks.
    pub(crate) fn activate(&self, handle: &ComponentHandle) {
        let mut pending = self.pending();
        let Some((activation, active)) = retain_current(&mut pending, handle) else {
            return;
        };
        if !active {
            return;
        }

        for (task, entry) in mem::take(&mut *pending) {
            let entry = match entry {
                Entry::Waiting(job) => self.start(handle, job),
                started @ Entry::Started { .. } => started,
            };
            pending.insert(task, entry);
        }

        for task in self.scheduler.periodic() {
            let job = Job::periodic(task, activation);
            if let Err(error) = self.insert(&mut pending, handle, job, true) {
                warn!(component = handle.id(), task = task.task, %error, "scheduling periodic task failed");
            }
        }
    }

    pub(crate) fn prune(&self, handle: &ComponentHandle) {
        retain_current(&mut self.pending(), handle);
    }

    fn schedule(
        &self,
        handle: &ComponentHandle,
        task: String,
        args: Vec<u8>,
        schedule: Schedule,
    ) -> Result<(), ScheduleError> {
        if args.len() > MAX_ARGS_SIZE {
            return Err(ScheduleError::ArgsTooLarge { max: MAX_ARGS_SIZE });
        }

        let mut pending = self.pending();
        let (activation, active) = handle.activation().map_err(ScheduleError::NotAllowed)?;
        let job = Job::new(task, args, schedule, activation);
        self.insert(&mut pending, handle, job, active)
    }

    /// Adds `job`, replacing the task with the same name if it already exists.
    /// If the component is `active` its timer starts immediately, otherwise
    /// on [`Self::activate`].
    fn insert(
        &self,
        pending: &mut Pending,
        handle: &ComponentHandle,
        job: Job,
        active: bool,
    ) -> Result<(), ScheduleError> {
        if !pending.contains_key(&job.task) && pending.len() >= MAX_TASKS {
            return Err(ScheduleError::TooMany { max: MAX_TASKS });
        }

        debug!(
            component = handle.id(),
            task = job.task,
            delay = ?job.schedule.delay,
            interval = ?job.schedule.interval,
            waiting = !active,
            "task scheduled"
        );
        let task = job.task.clone();
        let entry = match active {
            true => self.start(handle, job),
            false => Entry::Waiting(job),
        };
        pending.insert(task, entry);
        Ok(())
    }

    /// Starts the timer of `job`, cancelled when the returned entry is dropped.
    fn start(&self, handle: &ComponentHandle, job: Job) -> Entry {
        let activation = job.activation;
        let (cancel, cancelled) = oneshot::channel();
        self.tokio_handle
            .spawn(timer(handle.downgrade(), job, cancelled));
        Entry::Started {
            activation,
            _cancel: cancel,
        }
    }

    fn cancel(&self, task: &str) {
        self.pending().remove(task);
    }

    /// Removes a due one-shot task unless it was cancelled or replaced in the
    /// meantime. Returns whether it should still run.
    fn remove_due(&self, task: &str, cancelled: &mut oneshot::Receiver<()>) -> bool {
        // Cancelling and replacing drop the entry under the lock, so checking
        // under it too ensures we never remove a replacement.
        let mut pending = self.pending();
        let is_current = cancelled.try_recv() == Err(TryRecvError::Empty);
        if is_current {
            pending.remove(task);
        }
        is_current
    }

    async fn run(&self, handle: &ComponentHandle, job: &Job) {
        let (component, task) = (handle.id(), &job.task);
        let permit = match Permit::begin_in(handle, job.activation, SchedulerOperation::Run) {
            Ok(permit) => permit,
            Err(error) => {
                debug!(component, task, %error, "scheduled task skipped");
                return;
            }
        };

        debug!(component, task, "running scheduled task");
        let result = self.scheduler.run(task, &job.args).await;
        if let Err(error) = permit.finish(result).await {
            debug!(component, task, %error, "finishing scheduled task failed");
        }
    }

    fn pending(&self) -> MutexGuard<'_, Pending> {
        self.pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Drops tasks from past activations, returning the current activation and
/// whether the component is active.
fn retain_current(pending: &mut Pending, handle: &ComponentHandle) -> Option<(Activation, bool)> {
    let current = handle.activation().ok();
    pending
        .retain(|_, entry| current.is_some_and(|(activation, _)| entry.activation() == activation));
    current
}

/// Runs `job` on its schedule until `cancelled`. Cancelling only stops the
/// waits and never an actual run midway.
async fn timer(handle: WeakComponentHandle, job: Job, mut cancelled: oneshot::Receiver<()>) {
    let mut delay = job.schedule.delay;
    loop {
        select! {
            biased;
            _ = &mut cancelled => return,
            () = sleep(delay) => {}
        }

        let Some(handle) = handle.upgrade() else {
            return;
        };
        let Some(timers) = handle.timers() else {
            return;
        };

        let Some(interval) = job.schedule.interval else {
            if timers.remove_due(&job.task, &mut cancelled) {
                timers.run(&handle, &job).await;
            }
            return;
        };
        timers.run(&handle, &job).await;
        delay = interval;
    }
}
