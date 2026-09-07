use super::error::{OperationError, Result};
use crate::{ComponentHandle, StorefrontOperation};
use domain::component::Status;
use event::Topic;
use strum::Display;
use tracing::debug;

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

    /// Exclusive operations run alone. Shared ones run alongside other shared
    /// operations, but never alongside an exclusive one.
    fn is_exclusive(self) -> bool {
        match self {
            Self::Init | Self::Login | Self::Logout => true,
            Self::Configure => false,
            Self::Storefront(operation) => operation.is_exclusive(),
        }
    }
}

/// `Ok` if an operation could start now, otherwise the operation that blocks it.
type Available = std::result::Result<(), Operation>;

#[derive(Debug, Default)]
pub(crate) struct InFlight {
    exclusive: Option<Operation>,
    shared: Vec<Operation>,
}

impl InFlight {
    pub(super) fn running(&self) -> Vec<Operation> {
        self.exclusive
            .into_iter()
            .chain(self.shared.iter().copied())
            .collect()
    }

    pub(super) fn is_running(&self, operation: Operation) -> bool {
        self.exclusive == Some(operation) || self.shared.contains(&operation)
    }

    pub(super) fn accepts(&self, operation: Operation) -> bool {
        self.check(operation).is_ok()
    }

    /// Adds `operation` to the in-flight set, or returns the operation that
    /// currently blocks it.
    pub(super) fn acquire(&mut self, operation: Operation) -> Available {
        self.check(operation)?;
        self.place(operation);
        Ok(())
    }

    /// Moves the hold from `from` to `to`, restoring `from` if `to` is blocked.
    fn swap(&mut self, from: Operation, to: Operation) -> Available {
        self.remove(from);
        if let Err(blocked_by) = self.acquire(to) {
            self.place(from);
            return Err(blocked_by);
        }
        Ok(())
    }

    fn release(&mut self, operation: Operation) {
        debug_assert!(
            self.remove(operation),
            "released {operation} which was not held"
        );
    }

    /// `Ok` if `operation` could start now, otherwise the operation that
    /// blocks it.
    fn check(&self, operation: Operation) -> Available {
        let blocker = if operation.is_exclusive() {
            self.blocking()
        } else {
            self.exclusive
        };
        match blocker {
            Some(blocker) => Err(blocker),
            None => Ok(()),
        }
    }

    fn blocking(&self) -> Option<Operation> {
        self.exclusive.or_else(|| self.shared.first().copied())
    }

    fn place(&mut self, operation: Operation) {
        if operation.is_exclusive() {
            self.exclusive = Some(operation);
        } else {
            self.shared.push(operation);
        }
    }

    /// Removes one instance of `operation`, returning whether it was held.
    fn remove(&mut self, operation: Operation) -> bool {
        if self.exclusive == Some(operation) {
            self.exclusive = None;
            true
        } else if let Some(index) = self.shared.iter().position(|o| *o == operation) {
            self.shared.remove(index);
            true
        } else {
            false
        }
    }
}

/// Holds a component while an operation is running.
///
/// Most operations create and drop a permit internally. Interactive operations
/// such as login or logout should return a permit to the caller so the component
/// stays held during the flow.
pub struct Permit {
    handle: ComponentHandle,
    operation: Operation,
}

impl Permit {
    pub(super) fn new(handle: ComponentHandle, operation: Operation) -> Self {
        Self { handle, operation }
    }

    /// Re-tags the permit for `operation` and checks the status still allows it.
    pub(super) fn enter(&mut self, operation: impl Into<Operation>) -> Result<()> {
        let operation = operation.into();
        self.retag(operation)?;

        let status = self.handle.status();
        if !operation.allowed_by(&status) {
            debug!(component = self.handle.id(), %operation, %status, "operation rejected");
            return Err(OperationError::NotAllowed { operation, status });
        }
        Ok(())
    }

    /// Moves the hold from the current operation to `operation`, restoring the
    /// original tag if the new one cannot be acquired.
    fn retag(&mut self, operation: Operation) -> Result<()> {
        if self.operation == operation {
            return Ok(());
        }

        let swapped = {
            let mut state = self.handle.state_write();
            state.in_flight.swap(self.operation, operation)
        };

        if let Err(blocked_by) = swapped {
            debug!(
                component = self.handle.id(),
                %operation,
                %blocked_by,
                "cannot retag permit"
            );
            return Err(OperationError::Busy {
                operation,
                blocked_by,
            });
        }

        debug!(
            component = self.handle.id(),
            from = %self.operation,
            to = %operation,
            "permit retagged"
        );
        self.operation = operation;
        event::emit(Topic::Component);
        Ok(())
    }
}

impl Drop for Permit {
    fn drop(&mut self) {
        self.handle.state_write().in_flight.release(self.operation);

        debug!(
            component = self.handle.id(),
            operation = %self.operation,
            "permit released"
        );
        event::emit(Topic::Component);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INIT: Operation = Operation::Init;
    const LOGIN: Operation = Operation::Login;
    const LOGOUT: Operation = Operation::Logout;
    const CONFIGURE: Operation = Operation::Configure;
    const SYNC: Operation = Operation::Storefront(StorefrontOperation::Sync);

    #[track_caller]
    fn in_flight_with<const N: usize>(operations: [Operation; N]) -> InFlight {
        let mut in_flight = InFlight::default();
        for operation in operations {
            in_flight
                .acquire(operation)
                .unwrap_or_else(|blocker| panic!("{operation} unexpectedly blocked by {blocker}"));
        }
        in_flight
    }

    #[track_caller]
    fn check_acquire<const N: usize>(
        running: [Operation; N],
        operation: Operation,
        expected: Available,
    ) {
        assert_eq!(in_flight_with(running).acquire(operation), expected);
    }

    #[test]
    fn an_idle_component_accepts_any_operation() {
        for operation in [INIT, LOGIN, LOGOUT, CONFIGURE, SYNC] {
            check_acquire([], operation, Ok(()));
        }
    }

    #[test]
    fn an_exclusive_operation_runs_alone() {
        for blocked in [INIT, LOGIN, LOGOUT, CONFIGURE, SYNC] {
            check_acquire([LOGIN], blocked, Err(LOGIN));
        }
    }

    #[test]
    fn shared_operations_run_concurrently() {
        check_acquire([CONFIGURE], SYNC, Ok(()));
        check_acquire([SYNC], CONFIGURE, Ok(()));
        check_acquire([SYNC, CONFIGURE, SYNC], SYNC, Ok(()));
    }

    #[test]
    fn a_shared_operation_blocks_an_exclusive_one() {
        check_acquire([SYNC], INIT, Err(SYNC));
        check_acquire([CONFIGURE], LOGOUT, Err(CONFIGURE));
    }

    #[test]
    fn an_operation_can_start_once_its_blocker_finishes() {
        let mut in_flight = in_flight_with([LOGIN]);
        assert_eq!(in_flight.acquire(INIT), Err(LOGIN));

        in_flight.release(LOGIN);
        assert_eq!(in_flight.acquire(INIT), Ok(()));
    }

    #[test]
    fn an_exclusive_operation_waits_for_every_shared_operation() {
        let mut in_flight = in_flight_with([SYNC, SYNC]);

        in_flight.release(SYNC);
        assert_eq!(in_flight.acquire(LOGIN), Err(SYNC));

        in_flight.release(SYNC);
        assert_eq!(in_flight.acquire(LOGIN), Ok(()));
    }

    #[test]
    fn running_reports_every_operation_in_flight() {
        let in_flight = in_flight_with([SYNC, CONFIGURE]);

        assert_eq!(in_flight.running().len(), 2);
        assert!(in_flight.is_running(SYNC));
        assert!(in_flight.is_running(CONFIGURE));
        assert!(!in_flight.is_running(LOGIN));
    }

    #[test]
    fn re_tagging_a_hold_swaps_the_running_operation() {
        let mut in_flight = in_flight_with([INIT]);

        in_flight.swap(INIT, LOGIN).unwrap();

        assert!(in_flight.is_running(LOGIN));
        assert!(!in_flight.is_running(INIT));
    }

    #[test]
    fn a_re_tag_that_would_be_blocked_keeps_the_original_hold() {
        let mut in_flight = in_flight_with([SYNC, CONFIGURE]);

        assert_eq!(in_flight.swap(SYNC, LOGIN), Err(CONFIGURE));

        assert!(in_flight.is_running(SYNC));
        assert!(!in_flight.is_running(LOGIN));
    }
}
