use super::error::Result;
use crate::ComponentHandle;
use domain::component::{Activation, Error, Operation, Running};

/// Holds a component while an operation is running.
///
/// Component errors are reported through [`Permit::finish`], as they can
/// change the status. Any other failure drops the permit, which cancels the
/// operation.
pub(crate) struct Permit {
    handle: ComponentHandle,
    running: Option<Running>,
}

impl Permit {
    pub(crate) fn begin(handle: &ComponentHandle, operation: impl Into<Operation>) -> Result<Self> {
        let running = handle.start(operation.into(), None)?;
        Ok(Self::new(handle, running))
    }

    /// Like [`Permit::begin`], but only while `activation` is still current.
    pub(crate) fn begin_in(
        handle: &ComponentHandle,
        activation: Activation,
        operation: impl Into<Operation>,
    ) -> Result<Self> {
        let running = handle.start(operation.into(), Some(activation))?;
        Ok(Self::new(handle, running))
    }

    fn new(handle: &ComponentHandle, running: Running) -> Self {
        Self {
            handle: handle.clone(),
            running: Some(running),
        }
    }

    /// Reports the result of the operation, running any initialization it
    /// starts.
    pub(crate) async fn finish(mut self, result: std::result::Result<(), Error>) -> Result<()> {
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
