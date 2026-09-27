use crate::component::{Error, StorefrontOperation};
use strum::Display;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    capabilities: Capabilities,
    status: Status,
    in_flight: Operations,
}

impl State {
    pub fn new(capabilities: Capabilities) -> Self {
        Self {
            capabilities,
            status: Status::default(),
            in_flight: Operations::default(),
        }
    }

    pub fn status(&self) -> &Status {
        &self.status
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

    pub fn finish(&mut self, running: Running, result: Result<(), Error>) -> Outcome {
        let next = match (self.release(running), result) {
            (Operation::Login, Ok(())) => Status::Initializing,
            (Operation::Logout, Ok(())) => Status::Inactive(InactiveReason::Unauthenticated),
            (Operation::Config, Ok(()))
                if matches!(
                    self.status,
                    Status::Inactive(InactiveReason::Error(Error::Config(_)))
                ) =>
            {
                Status::Initializing
            }
            (Operation::Storefront(_), Err(error))
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
        Outcome {
            initialization: self.status.is_initializing().then_some(Initialization(())),
            change: Some((previous, self.status.clone())),
            transient_error: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Capabilities {
    pub login: bool,
    pub config: bool,
    pub storefront: bool,
}

impl Capabilities {
    fn supports(self, operation: Operation) -> bool {
        match operation {
            Operation::Login | Operation::Logout => self.login,
            Operation::Config => self.config,
            Operation::Storefront(_) => self.storefront,
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
            Operation::Login => matches!(
                self,
                Self::Inactive(
                    InactiveReason::Unauthenticated | InactiveReason::Error(Error::Auth(_))
                )
            ),
            Operation::Logout => matches!(
                self,
                Self::Active | Self::Inactive(InactiveReason::Error(_))
            ),
            Operation::Config => matches!(
                self,
                Self::Active
                    | Self::Inactive(
                        InactiveReason::Disabled
                            | InactiveReason::Unauthenticated
                            | InactiveReason::Error(Error::Auth(_) | Error::Config(_))
                    )
            ),
            Operation::Storefront(_) => self.is_active(),
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
    Login,
    Logout,
    Config,
    #[strum(to_string = "{0}")]
    Storefront(StorefrontOperation),
}

impl Operation {
    fn is_exclusive(self) -> bool {
        match self {
            Self::Login | Self::Logout => true,
            Self::Config | Self::Storefront(_) => false,
        }
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
        if operation.is_exclusive() {
            self.first()
        } else {
            self.0
                .iter()
                .copied()
                .find(|running| *running == operation || running.is_exclusive())
        }
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
