use crate::Error;
use crate::wit::scheduler::thrustr::plugin::timers;
use serde::{Serialize, de::DeserializeOwned};
use std::time::Duration;

pub trait Task: Sized {
    fn name(&self) -> &str;
    fn from_name(name: &str) -> Option<Self>;
}

#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be a task parameter",
    label = "not serializable",
    note = "task parameters must implement `serde::Serialize` and `serde::Deserialize`"
)]
pub trait TaskArg: Serialize + DeserializeOwned {}

impl<T: Serialize + DeserializeOwned> TaskArg for T {}

pub fn task_arg<T: TaskArg>() {}

pub fn schedule(
    task: &impl Task,
    args: &impl Serialize,
    delay: Duration,
    interval: Option<Duration>,
) -> Result<(), Error> {
    let args = postcard::to_allocvec(args).map_err(|e| {
        Error::other(format!(
            "encoding arguments of task `{}` failed: {e}",
            task.name()
        ))
    })?;
    timers::schedule(task.name(), &args, millis(delay), interval.map(millis))
}

pub fn cancel(task: &impl Task) {
    timers::cancel(task.name());
}

pub fn decode<A: DeserializeOwned>(task: &impl Task, args: &[u8]) -> Result<A, Error> {
    postcard::from_bytes(args).map_err(|e| {
        Error::other(format!(
            "decoding arguments of task `{}` failed: {e}",
            task.name()
        ))
    })
}

pub fn millis(duration: Duration) -> u64 {
    duration.as_millis().try_into().unwrap_or(u64::MAX)
}
