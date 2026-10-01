use crate::component::{
    AuthOperation, ConfigOperation, Error, SchedulerOperation, StorefrontOperation,
};
use strum::Display;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    capabilities: Capabilities,
    status: Status,
    in_flight: Operations,
    /// How many times the component started initializing.
    activations: u64,
}

impl State {
    pub fn new(capabilities: Capabilities) -> Self {
        Self {
            capabilities,
            status: Status::default(),
            in_flight: Operations::default(),
            activations: 0,
        }
    }

    pub fn status(&self) -> &Status {
        &self.status
    }

    pub fn activation(&self) -> Option<Activation> {
        (self.status.is_initializing() || self.status.is_active())
            .then_some(Activation(self.activations))
    }

    pub fn is_running(&self, operation: Operation) -> bool {
        self.in_flight.contains(operation)
    }

    /// Whether the component supports `operation` and its status permits it,
    /// ignoring what is already running.
    pub fn allows(&self, operation: Operation) -> bool {
        self.capabilities.supports(operation) && self.status.allows(operation)
    }

    /// Whether `operation` could start right now.
    pub fn can(&self, operation: Operation) -> bool {
        self.check(operation).is_ok()
    }

    /// Whether a failed component could be reinitialized right now.
    pub fn can_reinitialize(&self) -> bool {
        self.check_reinitialize().is_ok()
    }

    // TODO: store a "logged-in" flag in database to differentiate between never
    // logged in and login error
    pub fn enable(&mut self) -> Result<Outcome, Rejection> {
        self.require(matches!(
            self.status,
            Status::Inactive(InactiveReason::Disabled)
        ))?;
        self.require_idle()?;
        Ok(self.set(Status::Initializing))
    }

    pub fn reinitialize(&mut self) -> Result<Outcome, Rejection> {
        self.check_reinitialize()?;
        Ok(self.set(Status::Initializing))
    }

    pub fn finish_initialization(
        &mut self,
        _: Initialization,
        result: Result<(), Error>,
    ) -> Outcome {
        debug_assert!(
            self.status.is_initializing(),
            "component should be initializing"
        );
        self.set(match result {
            Ok(()) => Status::Active,
            Err(error) => Status::Inactive(InactiveReason::Error(error)),
        })
    }

    pub fn start(&mut self, operation: Operation) -> Result<Running, Rejection> {
        self.check(operation)?;
        self.in_flight.insert(operation);
        Ok(Running(operation))
    }

    /// Starts `operation` on behalf of `activation`, which must be the current one.
    pub fn start_in(
        &mut self,
        activation: Activation,
        operation: Operation,
    ) -> Result<Running, Rejection> {
        if self.activation() != Some(activation) {
            return Err(Rejection::Expired);
        }
        self.start(operation)
    }

    pub fn finish(&mut self, running: Running, result: Result<(), Error>) -> Outcome {
        let next = match (self.release(running), result) {
            (Operation::Auth(AuthOperation::Login), Ok(())) => Status::Initializing,
            (Operation::Auth(AuthOperation::Logout), Ok(())) => {
                Status::Inactive(InactiveReason::Unauthenticated)
            }
            (Operation::Config(ConfigOperation::Save), Ok(()))
                if matches!(
                    self.status,
                    Status::Inactive(InactiveReason::Error(Error::Config(_)))
                ) =>
            {
                Status::Initializing
            }
            (Operation::Storefront(_) | Operation::Scheduler(_), Err(error))
                if self.status.is_active() && error.is_fatal() =>
            {
                Status::Inactive(InactiveReason::Error(error))
            }
            (_, result) => {
                return Outcome {
                    transient_error: result.err(),
                    ..Outcome::default()
                };
            }
        };
        self.set(next)
    }

    pub fn cancel(&mut self, running: Running) {
        self.release(running);
    }

    fn check(&self, operation: Operation) -> Result<(), Rejection> {
        if !self.capabilities.supports(operation) {
            return Err(Rejection::Unsupported(operation));
        }
        self.require(self.status.allows(operation))?;
        match self.in_flight.blocker(operation) {
            Some(blocker) => Err(Rejection::Busy(blocker)),
            None => Ok(()),
        }
    }

    fn check_reinitialize(&self) -> Result<(), Rejection> {
        self.require(self.status.error().is_some())?;
        self.require_idle()
    }

    fn release(&mut self, Running(operation): Running) -> Operation {
        let removed = self.in_flight.remove(operation);
        debug_assert!(removed, "{operation} should be in flight");
        operation
    }

    fn require(&self, allowed: bool) -> Result<(), Rejection> {
        match allowed {
            true => Ok(()),
            false => Err(Rejection::NotAllowed(self.status.clone())),
        }
    }

    fn require_idle(&self) -> Result<(), Rejection> {
        match self.in_flight.first() {
            Some(operation) => Err(Rejection::Busy(operation)),
            None => Ok(()),
        }
    }

    fn set(&mut self, status: Status) -> Outcome {
        if self.status == status {
            return Outcome::default();
        }
        let previous = std::mem::replace(&mut self.status, status);
        if self.status.is_initializing() {
            self.activations += 1;
        }
        Outcome {
            initialization: self.status.is_initializing().then_some(Initialization(())),
            change: Some((previous, self.status.clone())),
            transient_error: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Capabilities {
    pub auth: bool,
    pub config: bool,
    pub storefront: bool,
    pub scheduler: bool,
}

impl Capabilities {
    fn supports(self, operation: Operation) -> bool {
        match operation {
            Operation::Auth(_) => self.auth,
            Operation::Config(_) => self.config,
            Operation::Storefront(_) => self.storefront,
            Operation::Scheduler(_) => self.scheduler,
        }
    }
}

#[must_use]
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    /// The status before and after the transition, if it changed.
    pub change: Option<(Status, Status)>,
    /// A failure to report that did not affect the status.
    pub transient_error: Option<Error>,
    /// Set when the transition started an initialization. The caller must run
    /// it and report back with [`State::finish_initialization`].
    pub initialization: Option<Initialization>,
}

#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub struct Initialization(());

/// One run of the component, from initialization to inactive.
/// Scheduled tasks end with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Activation(u64);

/// An operation started with [`State::start`], to be reported back with
/// [`State::finish`] or [`State::cancel`].
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub struct Running(Operation);

impl Running {
    pub fn operation(&self) -> Operation {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Rejection {
    #[error("{0} is not supported by the component")]
    Unsupported(Operation),

    #[error("not allowed while the component is {0}")]
    NotAllowed(Status),

    #[error("{0} is running")]
    Busy(Operation),

    #[error("it belongs to an activation that has ended")]
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Display)]
#[strum(serialize_all = "lowercase")]
pub enum Status {
    #[strum(to_string = "inactive ({0})")]
    Inactive(InactiveReason),
    Initializing,
    Active,
}

#[derive(Debug, Clone, PartialEq, Eq, Display)]
#[strum(serialize_all = "lowercase")]
pub enum InactiveReason {
    // TODO: components are always enabled on registration. Add a `disable`
    // transition once there is enabling/disabling support.
    Disabled,
    Unauthenticated,
    #[strum(to_string = "{0}")]
    Error(Error),
}

impl Status {
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }

    pub fn is_initializing(&self) -> bool {
        matches!(self, Self::Initializing)
    }

    pub fn error(&self) -> Option<&Error> {
        match self {
            Self::Inactive(InactiveReason::Error(error)) => Some(error),
            _ => None,
        }
    }

    fn allows(&self, operation: Operation) -> bool {
        match operation {
            Operation::Auth(AuthOperation::Login) => matches!(
                self,
                Self::Inactive(
                    InactiveReason::Unauthenticated | InactiveReason::Error(Error::Auth(_))
                )
            ),
            Operation::Auth(AuthOperation::Logout) => matches!(
                self,
                Self::Active | Self::Inactive(InactiveReason::Error(_))
            ),
            Operation::Config(ConfigOperation::Save) => matches!(
                self,
                Self::Active
                    | Self::Inactive(
                        InactiveReason::Disabled
                            | InactiveReason::Unauthenticated
                            | InactiveReason::Error(Error::Auth(_) | Error::Config(_))
                    )
            ),
            Operation::Storefront(_) | Operation::Scheduler(_) => self.is_active(),
        }
    }
}

impl Default for Status {
    fn default() -> Self {
        Self::Inactive(InactiveReason::Disabled)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Display)]
#[strum(serialize_all = "lowercase")]
pub enum Operation {
    #[strum(to_string = "{0}")]
    Auth(AuthOperation),
    #[strum(to_string = "{0}")]
    Config(ConfigOperation),
    #[strum(to_string = "{0}")]
    Storefront(StorefrontOperation),
    #[strum(to_string = "{0}")]
    Scheduler(SchedulerOperation),
}

impl Operation {
    fn is_exclusive(self) -> bool {
        match self {
            Self::Auth(_) => true,
            Self::Config(_) | Self::Storefront(_) | Self::Scheduler(_) => false,
        }
    }

    /// Whether the operation may run alongside itself.
    fn is_reentrant(self) -> bool {
        matches!(
            self,
            Self::Storefront(StorefrontOperation::ListVersions) | Self::Scheduler(_)
        )
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Operations(Vec<Operation>);

impl Operations {
    fn first(&self) -> Option<Operation> {
        self.0.first().copied()
    }

    fn contains(&self, operation: Operation) -> bool {
        self.0.contains(&operation)
    }

    fn blocker(&self, operation: Operation) -> Option<Operation> {
        self.0.iter().copied().find(|&running| {
            operation.is_exclusive()
                || running.is_exclusive()
                || (running == operation && !operation.is_reentrant())
        })
    }

    fn insert(&mut self, operation: Operation) {
        self.0.push(operation);
    }

    fn remove(&mut self, operation: Operation) -> bool {
        match self.0.iter().position(|running| *running == operation) {
            Some(index) => {
                self.0.swap_remove(index);
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Step::{Enable, Initialize, Reinitialize, Run};

    const LOGIN: Operation = Operation::Auth(AuthOperation::Login);
    const LOGOUT: Operation = Operation::Auth(AuthOperation::Logout);
    const CONFIG: Operation = Operation::Config(ConfigOperation::Save);
    const SYNC: Operation = Operation::Storefront(StorefrontOperation::Sync);
    const VERSIONS: Operation = Operation::Storefront(StorefrontOperation::ListVersions);
    const TASK: Operation = Operation::Scheduler(SchedulerOperation::Run);
    const OPERATIONS: [Operation; 6] = [LOGIN, LOGOUT, CONFIG, SYNC, VERSIONS, TASK];

    const ALL: Capabilities = Capabilities {
        auth: true,
        config: true,
        storefront: true,
        scheduler: true,
    };

    fn auth() -> Error {
        Error::Auth("expired".into())
    }

    fn config() -> Error {
        Error::Config("bad path".into())
    }

    fn other() -> Error {
        Error::Other("network".into())
    }

    fn errors() -> [Error; 3] {
        [auth(), config(), other()]
    }

    fn disabled() -> Status {
        Status::Inactive(InactiveReason::Disabled)
    }

    fn unauthenticated() -> Status {
        Status::Inactive(InactiveReason::Unauthenticated)
    }

    fn failed(error: Error) -> Status {
        Status::Inactive(InactiveReason::Error(error))
    }

    #[track_caller]
    fn state_at(capabilities: Capabilities, status: &Status, running: &[Operation]) -> State {
        let mut state = State::new(capabilities);
        match status {
            Status::Inactive(InactiveReason::Disabled) => {}
            Status::Initializing => {
                let _ = state
                    .enable()
                    .expect("a disabled component should be enabled");
            }
            Status::Active => initialize(&mut state, Ok(())),
            Status::Inactive(InactiveReason::Error(error)) => {
                initialize(&mut state, Err(error.clone()))
            }
            Status::Inactive(InactiveReason::Unauthenticated) => {
                initialize(&mut state, Ok(()));
                let logout = state
                    .start(LOGOUT)
                    .expect("an active component should log out");
                let _ = state.finish(logout, Ok(()));
            }
        }
        assert_eq!(state.status(), status, "harness should reach the status");

        for &operation in running {
            let _ = state
                .start(operation)
                .unwrap_or_else(|rejection| panic!("{operation} should start: {rejection}"));
        }
        state
    }

    #[track_caller]
    fn initialize(state: &mut State, result: Result<(), Error>) {
        let initialization = state
            .enable()
            .expect("a disabled component should be enabled")
            .initialization
            .expect("enabling should start an initialization");
        let _ = state.finish_initialization(initialization, result);
    }

    #[track_caller]
    fn check_allows(status: Status, expected: &[Operation]) {
        let state = state_at(ALL, &status, &[]);
        for operation in OPERATIONS {
            let allowed = expected.contains(&operation);
            assert_eq!(
                state.allows(operation),
                allowed,
                "{operation} while {status}"
            );
            assert_eq!(
                state.can(operation),
                allowed,
                "{operation} while idle and {status}"
            );
        }
    }

    #[track_caller]
    fn check_start(
        capabilities: Capabilities,
        status: Status,
        running: &[Operation],
        operation: Operation,
        expected: Result<(), Rejection>,
    ) {
        let mut state = state_at(capabilities, &status, running);
        assert_eq!(state.can(operation), expected.is_ok(), "can {operation}");
        assert_eq!(state.start(operation).map(|_| ()), expected);
    }

    #[track_caller]
    fn check_finish(
        status: Status,
        operation: Operation,
        result: Result<(), Error>,
        expected: Status,
    ) {
        let mut state = state_at(ALL, &status, &[]);
        let running = state
            .start(operation)
            .unwrap_or_else(|rejection| panic!("{operation} should start: {rejection}"));
        let error = result.clone().err();
        let outcome = state.finish(running, result);

        assert_eq!(state.status(), &expected);
        assert!(
            !state.is_running(operation),
            "{operation} should have finished"
        );
        assert_eq!(
            outcome.initialization.is_some(),
            expected.is_initializing(),
            "whether an initialization started"
        );

        let changed = status != expected;
        assert_eq!(outcome.transient_error, if changed { None } else { error });
        assert_eq!(outcome.change, changed.then_some((status, expected)));
    }

    #[track_caller]
    fn check_enable(status: Status, running: &[Operation], expected: Result<(), Rejection>) {
        let mut state = state_at(ALL, &status, running);
        let outcome = state.enable();
        check_initialization_started(&state, status, outcome, expected);
    }

    #[track_caller]
    fn check_reinitialize(status: Status, running: &[Operation], expected: Result<(), Rejection>) {
        let mut state = state_at(ALL, &status, running);
        assert_eq!(
            state.can_reinitialize(),
            expected.is_ok(),
            "can reinitialize"
        );
        let outcome = state.reinitialize();
        check_initialization_started(&state, status, outcome, expected);
    }

    #[track_caller]
    fn check_initialization_started(
        state: &State,
        before: Status,
        outcome: Result<Outcome, Rejection>,
        expected: Result<(), Rejection>,
    ) {
        match (outcome, expected) {
            (Ok(outcome), Ok(())) => {
                assert!(
                    outcome.initialization.is_some(),
                    "initialization should start"
                );
                assert_eq!(outcome.change, Some((before, Status::Initializing)));
                assert_eq!(state.status(), &Status::Initializing);
            }
            (outcome, expected) => {
                assert_eq!(outcome.map(|_| ()), expected);
                assert_eq!(state.status(), &before);
            }
        }
    }

    #[track_caller]
    fn check_activated(status: Status, expected: bool) {
        let state = state_at(ALL, &status, &[]);
        assert_eq!(state.activation().is_some(), expected, "while {status}");
    }

    enum Step {
        Enable,
        Reinitialize,
        Initialize(Result<(), Error>),
        Run(Operation, Result<(), Error>),
    }

    #[track_caller]
    fn check_start_in(before: &[Step], after: &[Step], expected: Result<(), Rejection>) {
        let mut state = State::new(ALL);
        let mut pending = None;
        take_steps(&mut state, &mut pending, before);
        let activation = state
            .activation()
            .unwrap_or_else(|| panic!("{} should be activated", state.status()));
        take_steps(&mut state, &mut pending, after);
        assert_eq!(state.start_in(activation, TASK).map(|_| ()), expected);
    }

    #[track_caller]
    fn take_steps(state: &mut State, pending: &mut Option<Initialization>, steps: &[Step]) {
        for step in steps {
            let outcome = match step {
                Enable => state.enable().expect("the component should be enabled"),
                Reinitialize => state
                    .reinitialize()
                    .expect("the component should reinitialize"),
                Initialize(result) => {
                    let initialization =
                        pending.take().expect("an initialization should be pending");
                    state.finish_initialization(initialization, result.clone())
                }
                Run(operation, result) => {
                    let running = state.start(*operation).unwrap_or_else(|rejection| {
                        panic!("{operation} should start: {rejection}")
                    });
                    state.finish(running, result.clone())
                }
            };
            if let Some(initialization) = outcome.initialization {
                *pending = Some(initialization);
            }
        }
    }

    #[test]
    fn status_determines_allowed_operations() {
        check_allows(disabled(), &[CONFIG]);
        check_allows(Status::Initializing, &[]);
        check_allows(Status::Active, &[LOGOUT, CONFIG, SYNC, VERSIONS, TASK]);
        check_allows(unauthenticated(), &[LOGIN, CONFIG]);
        check_allows(failed(auth()), &[LOGIN, LOGOUT, CONFIG]);
        check_allows(failed(config()), &[LOGOUT, CONFIG]);
        check_allows(failed(other()), &[LOGOUT]);
    }

    #[test]
    fn unsupported_operations_are_rejected() {
        let no_auth = Capabilities { auth: false, ..ALL };
        let no_config = Capabilities {
            config: false,
            ..ALL
        };
        let no_storefront = Capabilities {
            storefront: false,
            ..ALL
        };

        check_start(
            no_auth,
            failed(auth()),
            &[],
            LOGIN,
            Err(Rejection::Unsupported(LOGIN)),
        );
        check_start(
            no_auth,
            Status::Active,
            &[],
            LOGOUT,
            Err(Rejection::Unsupported(LOGOUT)),
        );
        check_start(
            no_config,
            Status::Active,
            &[],
            CONFIG,
            Err(Rejection::Unsupported(CONFIG)),
        );
        check_start(
            no_storefront,
            Status::Active,
            &[],
            SYNC,
            Err(Rejection::Unsupported(SYNC)),
        );
        check_start(
            no_storefront,
            disabled(),
            &[],
            SYNC,
            Err(Rejection::Unsupported(SYNC)),
        );
        check_start(
            Capabilities {
                scheduler: false,
                ..ALL
            },
            Status::Active,
            &[],
            TASK,
            Err(Rejection::Unsupported(TASK)),
        );
    }

    #[test]
    fn status_forbids_some_operations() {
        check_start(
            ALL,
            disabled(),
            &[],
            SYNC,
            Err(Rejection::NotAllowed(disabled())),
        );
        check_start(
            ALL,
            Status::Initializing,
            &[],
            CONFIG,
            Err(Rejection::NotAllowed(Status::Initializing)),
        );
    }

    #[test]
    fn exclusive_operations_run_alone() {
        check_start(
            ALL,
            unauthenticated(),
            &[LOGIN],
            CONFIG,
            Err(Rejection::Busy(LOGIN)),
        );
        for operation in [LOGOUT, CONFIG, SYNC, VERSIONS, TASK] {
            check_start(
                ALL,
                Status::Active,
                &[LOGOUT],
                operation,
                Err(Rejection::Busy(LOGOUT)),
            );
        }
    }

    #[test]
    fn shared_operations_block_exclusives() {
        check_start(
            ALL,
            unauthenticated(),
            &[CONFIG],
            LOGIN,
            Err(Rejection::Busy(CONFIG)),
        );
        check_start(
            ALL,
            Status::Active,
            &[SYNC],
            LOGOUT,
            Err(Rejection::Busy(SYNC)),
        );
        check_start(
            ALL,
            Status::Active,
            &[TASK],
            LOGOUT,
            Err(Rejection::Busy(TASK)),
        );
    }

    #[test]
    fn different_shared_operations_run_concurrently() {
        check_start(ALL, Status::Active, &[CONFIG], SYNC, Ok(()));
        check_start(ALL, Status::Active, &[SYNC], CONFIG, Ok(()));
        check_start(ALL, Status::Active, &[CONFIG, SYNC], TASK, Ok(()));
        check_start(ALL, Status::Active, &[TASK], SYNC, Ok(()));
        check_start(ALL, Status::Active, &[SYNC], VERSIONS, Ok(()));
        check_start(ALL, Status::Active, &[VERSIONS], SYNC, Ok(()));
    }

    #[test]
    fn reentrant_operations_run_alongside_others() {
        check_start(ALL, Status::Active, &[TASK, TASK], TASK, Ok(()));
        check_start(ALL, Status::Active, &[VERSIONS, VERSIONS], VERSIONS, Ok(()));
    }

    #[test]
    fn only_initializing_and_active_components_are_activated() {
        check_activated(disabled(), false);
        check_activated(Status::Initializing, true);
        check_activated(Status::Active, true);
        check_activated(unauthenticated(), false);
        for error in errors() {
            check_activated(failed(error), false);
        }
    }

    #[test]
    fn an_activation_spans_initialization_and_active() {
        let active = [Enable, Initialize(Ok(()))];
        check_start_in(&[Enable], &[Initialize(Ok(()))], Ok(()));
        check_start_in(&active, &[], Ok(()));
        check_start_in(
            &active,
            &[Run(SYNC, Err(other())), Run(CONFIG, Ok(()))],
            Ok(()),
        );
    }

    #[test]
    fn operations_expire_with_their_activation() {
        let active = [Enable, Initialize(Ok(()))];
        check_start_in(
            &[Enable],
            &[Initialize(Err(other()))],
            Err(Rejection::Expired),
        );
        check_start_in(&active, &[Run(SYNC, Err(auth()))], Err(Rejection::Expired));
        check_start_in(&active, &[Run(LOGOUT, Ok(()))], Err(Rejection::Expired));
    }

    #[test]
    fn new_activations_do_not_revive_expired_operations() {
        let active = [Enable, Initialize(Ok(()))];
        check_start_in(
            &[Enable],
            &[Initialize(Err(other())), Reinitialize, Initialize(Ok(()))],
            Err(Rejection::Expired),
        );
        check_start_in(
            &active,
            &[Run(SYNC, Err(auth())), Reinitialize, Initialize(Ok(()))],
            Err(Rejection::Expired),
        );
        check_start_in(
            &active,
            &[Run(LOGOUT, Ok(())), Run(LOGIN, Ok(())), Initialize(Ok(()))],
            Err(Rejection::Expired),
        );
    }

    #[test]
    fn a_shared_operation_never_runs_twice() {
        check_start(
            ALL,
            Status::Active,
            &[SYNC],
            SYNC,
            Err(Rejection::Busy(SYNC)),
        );
        check_start(
            ALL,
            Status::Active,
            &[CONFIG],
            CONFIG,
            Err(Rejection::Busy(CONFIG)),
        );
        check_start(
            ALL,
            Status::Active,
            &[CONFIG, SYNC],
            SYNC,
            Err(Rejection::Busy(SYNC)),
        );
    }

    #[test]
    fn cancelling_does_not_change_status() {
        let mut state = state_at(ALL, &Status::Active, &[]);
        let config = state.start(CONFIG).expect("config should start");
        let sync = state.start(SYNC).expect("sync should start");

        state.cancel(config);
        assert_eq!(state.start(LOGOUT).map(|_| ()), Err(Rejection::Busy(SYNC)));

        state.cancel(sync);
        assert!(!state.is_running(SYNC), "sync should no longer be running");
        assert!(state.can(LOGOUT), "logout should be possible once idle");
        assert_eq!(state.status(), &Status::Active);
    }

    #[test]
    fn logging_in_starts_init() {
        for status in [unauthenticated(), failed(auth())] {
            check_finish(status, LOGIN, Ok(()), Status::Initializing);
        }
    }

    #[test]
    fn a_failed_login_keeps_the_status() {
        for error in errors() {
            check_finish(
                unauthenticated(),
                LOGIN,
                Err(error.clone()),
                unauthenticated(),
            );
            check_finish(failed(auth()), LOGIN, Err(error), failed(auth()));
        }
    }

    #[test]
    fn logging_out_leaves_the_component_unauthenticated() {
        for status in [
            Status::Active,
            failed(auth()),
            failed(config()),
            failed(other()),
        ] {
            check_finish(status, LOGOUT, Ok(()), unauthenticated());
        }
    }

    #[test]
    fn a_failed_logout_keeps_the_status() {
        for error in errors() {
            check_finish(Status::Active, LOGOUT, Err(error.clone()), Status::Active);
            check_finish(failed(other()), LOGOUT, Err(error), failed(other()));
        }
    }

    #[test]
    fn saving_config_reinitializes_a_misconfigured_component() {
        check_finish(failed(config()), CONFIG, Ok(()), Status::Initializing);
    }

    #[test]
    fn saving_config_otherwise_keeps_the_status() {
        for status in [
            disabled(),
            unauthenticated(),
            failed(auth()),
            Status::Active,
        ] {
            check_finish(status.clone(), CONFIG, Ok(()), status);
        }
    }

    #[test]
    fn a_rejected_config_keeps_the_status() {
        for status in [Status::Active, failed(config())] {
            check_finish(status.clone(), CONFIG, Err(config()), status);
        }
    }

    #[test]
    fn auth_or_config_errors_deactivate_the_component() {
        for operation in [SYNC, VERSIONS, TASK] {
            for error in [auth(), config()] {
                check_finish(Status::Active, operation, Err(error.clone()), failed(error));
            }
        }
    }

    #[test]
    fn other_results_keep_the_component_active() {
        for operation in [SYNC, VERSIONS, TASK] {
            check_finish(Status::Active, operation, Ok(()), Status::Active);
            check_finish(Status::Active, operation, Err(other()), Status::Active);
        }
    }

    #[test]
    fn enabling_initializes_a_disabled_component() {
        check_enable(disabled(), &[], Ok(()));
    }

    #[test]
    fn only_an_idle_disabled_component_can_be_enabled() {
        for status in [
            Status::Initializing,
            Status::Active,
            unauthenticated(),
            failed(other()),
        ] {
            check_enable(status.clone(), &[], Err(Rejection::NotAllowed(status)));
        }
        check_enable(disabled(), &[CONFIG], Err(Rejection::Busy(CONFIG)));
    }

    #[test]
    fn a_failed_component_can_be_reinitialized() {
        for error in errors() {
            check_reinitialize(failed(error), &[], Ok(()));
        }
    }

    #[test]
    fn only_an_idle_failed_component_can_be_reinitialized() {
        for status in [
            disabled(),
            Status::Initializing,
            Status::Active,
            unauthenticated(),
        ] {
            check_reinitialize(status.clone(), &[], Err(Rejection::NotAllowed(status)));
        }
        check_reinitialize(failed(config()), &[CONFIG], Err(Rejection::Busy(CONFIG)));
        check_reinitialize(failed(auth()), &[LOGIN], Err(Rejection::Busy(LOGIN)));
    }
}
