use super::error::{OperationError, Result};
use crate::{ComponentHandle, StorefrontOperation};
use domain::component::Status;
use event::Topic;
use strum::Display;
use tracing::debug;

/// An operation that occupies a component while it runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Display)]
#[strum(serialize_all = "lowercase")]
pub enum Operation {
    #[strum(to_string = "initialization")]
    Init,
    Login,
    Logout,
    #[strum(to_string = "configuration")]
    Configure,
    #[strum(to_string = "{0}")]
    Storefront(StorefrontOperation),
}

impl Operation {
    /// Exclusive operations run alone. Shared ones run alongside other shared
    /// operations, but never alongside an exclusive one.
    fn is_exclusive(self) -> bool {
        match self {
            Self::Init | Self::Login | Self::Logout | Self::Configure => true,
            Self::Storefront(operation) => operation.is_exclusive(),
        }
    }

    /// Whether `status` permits this operation, ignoring what is already
    /// running.
    pub(super) fn allowed_by(self, status: &Status) -> bool {
        match self {
            Self::Init => status.can_init(),
            Self::Login => status.can_login(),
            Self::Logout => status.can_logout(),
            Self::Configure => status.can_configure(),
            Self::Storefront(_) => status.is_active(),
        }
    }
}

/// The operations running on a component right now.
#[derive(Debug, Default)]
pub(crate) struct InFlight {
    exclusive: Option<Operation>,
    shared: Vec<Operation>,
}

impl InFlight {
    pub(super) fn is_idle(&self) -> bool {
        self.exclusive.is_none() && self.shared.is_empty()
    }

    pub(super) fn exclusive(&self) -> Option<Operation> {
        self.exclusive
    }

    pub(super) fn blocking(&self) -> Option<Operation> {
        self.exclusive.or_else(|| self.shared.first().copied())
    }

    pub(super) fn accepts(&self, operation: Operation) -> bool {
        match operation.is_exclusive() {
            true => self.is_idle(),
            false => self.exclusive.is_none(),
        }
    }

    pub(super) fn acquire(&mut self, operation: Operation) -> bool {
        if !self.accepts(operation) {
            return false;
        }

        match operation.is_exclusive() {
            true => self.exclusive = Some(operation),
            false => self.shared.push(operation),
        }
        true
    }

    fn release(&mut self, operation: Operation) {
        if self.exclusive == Some(operation) {
            self.exclusive = None;
        } else if let Some(index) = self.shared.iter().position(|o| *o == operation) {
            self.shared.remove(index);
        }
    }

    #[cfg(test)]
    fn shared(&self) -> &[Operation] {
        &self.shared
    }
}

/// A component's permission to run one operation.
pub struct Claim {
    handle: ComponentHandle,
    operation: Operation,
}

impl Claim {
    pub(super) fn new(handle: ComponentHandle, operation: Operation) -> Self {
        Self { handle, operation }
    }

    /// Re-tags the claim for `operation`.
    pub(super) fn transition(&mut self, operation: Operation) -> Result<()> {
        if self.operation == operation {
            return Ok(());
        }

        let acquired = {
            let mut in_flight = self.handle.in_flight_write();
            in_flight.release(self.operation);

            match in_flight.acquire(operation) {
                true => Ok(()),
                false => {
                    let blocked_by = in_flight.blocking();
                    in_flight.acquire(self.operation);
                    Err(blocked_by)
                }
            }
        };

        if let Err(blocked_by) = acquired {
            debug!(
                component = self.handle.id(),
                %operation,
                blocked_by = blocked_by.map(display),
                "cannot upgrade claim"
            );
            return Err(OperationError::Busy { operation });
        }

        debug!(
            component = self.handle.id(),
            from = %self.operation,
            to = %operation,
            "claim moved"
        );
        self.operation = operation;
        event::emit(Topic::Component);
        Ok(())
    }
}

impl Drop for Claim {
    fn drop(&mut self) {
        self.handle.in_flight_write().release(self.operation);

        debug!(
            component = self.handle.id(),
            operation = %self.operation,
            "claim released"
        );
        event::emit(Topic::Component);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A shared operation, borrowed from the storefront family.
    fn sync() -> Operation {
        Operation::Storefront(StorefrontOperation::Sync)
    }

    #[test]
    fn capability_operations_display_as_themselves() {
        assert_eq!(sync().to_string(), "game sync");
        assert_eq!(Operation::Init.to_string(), "initialization");
        assert_eq!(Operation::Login.to_string(), "login");
    }

    #[test]
    fn exclusive_runs_alone() {
        let mut in_flight = InFlight::default();
        assert!(in_flight.acquire(Operation::Login));

        assert!(!in_flight.acquire(Operation::Logout));
        assert!(!in_flight.acquire(sync()));
        assert_eq!(in_flight.blocking(), Some(Operation::Login));
    }

    #[test]
    fn shared_runs_alongside_shared() {
        let mut in_flight = InFlight::default();
        assert!(in_flight.acquire(sync()));
        assert!(in_flight.acquire(sync()));

        assert_eq!(in_flight.shared().len(), 2);
        assert!(in_flight.exclusive().is_none());
    }

    #[test]
    fn shared_blocks_exclusive() {
        let mut in_flight = InFlight::default();
        in_flight.acquire(sync());

        assert!(!in_flight.acquire(Operation::Login));
        assert!(!in_flight.is_idle());
        assert_eq!(in_flight.blocking(), Some(sync()));
    }

    #[test]
    fn release_drops_one_shared_instance() {
        let mut in_flight = InFlight::default();
        in_flight.acquire(sync());
        in_flight.acquire(sync());

        in_flight.release(sync());
        assert_eq!(in_flight.shared(), [sync()]);

        in_flight.release(sync());
        assert!(in_flight.is_idle());
    }

    #[test]
    fn exclusive_blocked_while_any_shared_remains() {
        let mut in_flight = InFlight::default();
        in_flight.acquire(sync());
        in_flight.acquire(sync());

        in_flight.release(sync());
        assert!(!in_flight.acquire(Operation::Login));

        assert!(in_flight.acquire(sync()));
        assert_eq!(in_flight.shared().len(), 2);
    }
}
