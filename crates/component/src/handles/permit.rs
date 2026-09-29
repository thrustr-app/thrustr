use super::error::Result;
use crate::ComponentHandle;
use domain::component::{Error, Operation, Running};

/// Holds a component while an operation is running.
///
/// Component errors are reported through [`Permit::finish`], as they can
/// change the status. Any other failure drops the permit, which cancels the
/// operation.
pub(super) struct Permit {
    handle: ComponentHandle,
    running: Option<Running>,
}

impl Permit {
    pub(super) fn begin(handle: &ComponentHandle, operation: impl Into<Operation>) -> Result<Self> {
        let running = handle.start(operation.into())?;
        Ok(Self {
            handle: handle.clone(),
            running: Some(running),
        })
    }

    /// Reports the result of the operation, running any initialization it
    /// starts.
    pub(super) async fn finish(mut self, result: std::result::Result<(), Error>) -> Result<()> {
        let running = self
            .running
            .take()
            .expect("permit should hold its operation until finished");
        let outcome = self.handle.state_write().finish(running, result);
        self.handle.report(&outcome);
        self.handle.initialize(outcome.initialization).await
    }
}

impl Drop for Permit {
    fn drop(&mut self) {
        if let Some(running) = self.running.take() {
            self.handle.cancel(running);
        }
    }
}
