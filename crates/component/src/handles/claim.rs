use crate::{ComponentHandle, StorefrontOperation};
use std::sync::{RwLockReadGuard, RwLockWriteGuard};
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
}

/// The operations running on a component right now.
#[derive(Debug, Default)]
pub(crate) struct InFlight {
    exclusive: Option<Operation>,
    shared: Vec<Operation>,
}

impl InFlight {
    pub fn is_idle(&self) -> bool {
        self.exclusive.is_none() && self.shared.is_empty()
    }

    pub fn exclusive(&self) -> Option<Operation> {
        self.exclusive
    }

    #[cfg(test)]
    fn shared(&self) -> &[Operation] {
        &self.shared
    }

    pub fn blocking(&self) -> Option<Operation> {
        self.exclusive.or_else(|| self.shared.first().copied())
    }

    fn accepts(&self, operation: Operation) -> bool {
        match operation.is_exclusive() {
            true => self.is_idle(),
            false => self.exclusive.is_none(),
        }
    }

    fn acquire(&mut self, operation: Operation) -> bool {
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
}

/// A component's permission to run one operation.
pub struct Claim {
    handle: ComponentHandle,
    operation: Operation,
}

impl Claim {
    pub(super) fn transition(&mut self, operation: Operation) -> Result<(), String> {
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
            return Err(format!(
                "Cannot start {operation} while another operation is running"
            ));
        }

        debug!(
            component = self.handle.id(),
            from = %self.operation,
            to = %operation,
            "claim moved"
        );
        self.operation = operation;
        event::emit("component");
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
        event::emit("component");
    }
}

impl ComponentHandle {
    /// The exclusive operation running on this component, if any.
    pub fn running(&self) -> Option<Operation> {
        self.in_flight_read().exclusive()
    }

    /// Whether `operation` could be started right now.
    pub fn can(&self, operation: Operation) -> bool {
        self.allows(operation) && self.in_flight_read().accepts(operation)
    }

    /// Claims the component for `operation`. `None` if the status forbids it or
    /// the component is busy with something incompatible.
    pub fn begin(&self, operation: Operation) -> Option<Claim> {
        if !self.allows(operation) {
            debug!(
                component = self.id(),
                %operation,
                status = %self.status(),
                "operation rejected"
            );
            return None;
        }

        let acquired = {
            let mut in_flight = self.in_flight_write();
            match in_flight.acquire(operation) {
                true => Ok(()),
                false => Err(in_flight.blocking()),
            }
        };

        if let Err(busy_with) = acquired {
            debug!(
                component = self.id(),
                %operation,
                busy_with = busy_with.map(display),
                "component is busy"
            );
            return None;
        }

        debug!(component = self.id(), %operation, "claim acquired");
        event::emit("component");
        Some(Claim {
            handle: self.clone(),
            operation,
        })
    }

    /// Whether the status allows `operation`, ignoring what is already running.
    fn allows(&self, operation: Operation) -> bool {
        let status = self.status();
        match operation {
            Operation::Init => status.can_init(),
            Operation::Login => status.can_login(),
            Operation::Logout => status.can_logout(),
            Operation::Configure => status.can_configure(),
            Operation::Storefront(_) => status.is_active(),
        }
    }

    fn in_flight_read(&self) -> RwLockReadGuard<'_, InFlight> {
        self.in_flight
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn in_flight_write(&self) -> RwLockWriteGuard<'_, InFlight> {
        self.in_flight
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
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
    fn downgrade_to_shared_always_succeeds() {
        let mut in_flight = InFlight::default();
        in_flight.acquire(Operation::Init);

        in_flight.release(Operation::Init);
        assert!(in_flight.acquire(sync()));

        assert!(in_flight.exclusive().is_none());
        assert_eq!(in_flight.shared(), [sync()]);
    }

    #[test]
    fn upgrade_fails_while_other_shared_run() {
        let mut in_flight = InFlight::default();
        in_flight.acquire(sync());
        in_flight.acquire(sync());

        in_flight.release(sync());
        assert!(!in_flight.acquire(Operation::Login));

        assert!(in_flight.acquire(sync()));
        assert_eq!(in_flight.shared().len(), 2);
    }

    #[test]
    fn upgrade_succeeds_when_last_shared() {
        let mut in_flight = InFlight::default();
        in_flight.acquire(sync());

        in_flight.release(sync());
        assert!(in_flight.acquire(Operation::Login));
        assert_eq!(in_flight.exclusive(), Some(Operation::Login));
    }
}
