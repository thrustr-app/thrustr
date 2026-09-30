use crate::component::{Error, Operation, Status};
use async_trait::async_trait;
use std::time::Duration;
use strum::Display;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Display)]
#[strum(serialize_all = "lowercase")]
pub enum SchedulerOperation {
    #[strum(to_string = "scheduled task")]
    Run,
}

impl From<SchedulerOperation> for Operation {
    fn from(operation: SchedulerOperation) -> Self {
        Self::Scheduler(operation)
    }
}

#[async_trait]
pub trait Scheduler: Send + Sync {
    fn periodic(&self) -> &[PeriodicTask];
    async fn run(&self, task: &str, args: &[u8]) -> Result<(), Error>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeriodicTask {
    pub task: String,
    pub interval: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Schedule {
    pub delay: Duration,
    pub interval: Option<Duration>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScheduleError {
    #[error("the component does not run scheduled tasks")]
    Unsupported,

    #[error("tasks cannot be scheduled while the component is {0}")]
    NotAllowed(Status),

    #[error("a component cannot have more than {max} pending tasks")]
    TooMany { max: usize },

    #[error("task arguments cannot exceed {max} bytes")]
    ArgsTooLarge { max: usize },
}
