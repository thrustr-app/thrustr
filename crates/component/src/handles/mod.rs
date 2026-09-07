use self::error::Result;
use crate::{RegistryContext, handles::permit::InFlight};
use domain::component::{
    AuthFlow, Component, ComponentConfig, LoginMethod, LoginRequest, Metadata, Status, StatusEvent,
};
use event::Topic;
use std::{
    collections::HashMap,
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
};
use tracing::{debug, info, warn};

mod error;
mod permit;
mod storefront;

pub use error::OperationError;
pub use permit::{Operation, Permit};
pub use storefront::{StorefrontHandle, StorefrontOperation};

#[derive(Default)]
struct State {
    status: Status,
    in_flight: InFlight,
}

#[derive(Clone)]
pub struct ComponentHandle {
    component: Arc<dyn Component>,
    context: RegistryContext,
    state: Arc<RwLock<State>>,
}

impl ComponentHandle {
    pub fn new(component: Arc<dyn Component>, context: RegistryContext) -> Self {
        Self {
            component,
            context,
            state: Arc::new(RwLock::new(State::default())),
        }
    }

    pub fn id(&self) -> &str {
        self.component.metadata().id
    }

    pub fn metadata(&self) -> Metadata<'_> {
        self.component.metadata()
    }

    pub fn status(&self) -> Status {
        self.state_read().status.clone()
    }

    pub fn config(&self) -> Option<ComponentConfig> {
        self.component.config()
    }

    pub fn storefront(&self) -> Option<StorefrontHandle> {
        Arc::clone(&self.component)
            .storefront()
            .map(|storefront| StorefrontHandle::new(storefront, self.clone()))
    }

    pub async fn login_method(&self) -> Result<Option<LoginMethod>> {
        Ok(self.component.login_method().await?)
    }

    pub async fn logout_flow(&self) -> Result<Option<AuthFlow>> {
        Ok(self.component.logout_flow().await?)
    }

    pub async fn validate_config(&self, fields: HashMap<String, String>) -> Result<()> {
        Ok(self.component.validate_config(fields).await?)
    }

    pub fn config_values(&self) -> Result<HashMap<String, String>> {
        Ok(self
            .context
            .component_storage
            .get_config_values(self.id())?)
    }

    pub fn running(&self) -> Vec<Operation> {
        self.state_read().in_flight.running()
    }

    pub fn is_running(&self, operation: Operation) -> bool {
        self.state_read().in_flight.is_running(operation)
    }

    /// Whether `operation` could be started right now.
    pub fn can(&self, operation: Operation) -> bool {
        let state = self.state_read();
        operation.allowed_by(&state.status) && state.in_flight.accepts(operation)
    }

    pub async fn init(&self) -> Result<()> {
        let mut permit = self.reserve(Operation::Init)?;
        self.run_init(&mut permit).await
    }

    pub async fn save_config(&self, fields: HashMap<String, String>) -> Result<()> {
        let mut permit = self.reserve(Operation::Configure)?;

        self.validate_config(fields.clone()).await?;
        self.context
            .component_storage
            .set_config_values(self.id(), &fields)
            .map_err(|e| {
                warn!(component = self.id(), error = %e, "storing configuration failed");
                e
            })?;

        info!(component = self.id(), "configuration saved");

        let status =
            self.transition(StatusEvent::ConfigSaved)
                .ok_or(OperationError::StatusChanged {
                    operation: Operation::Configure,
                })?;

        self.init_if_ready(&mut permit, status).await
    }

    /// Reserves the component for an interactive login.
    pub fn begin_login(&self) -> Result<Permit> {
        self.reserve(Operation::Login)
    }

    /// Completes an interactive login and initializes the component if needed.
    pub async fn login(&self, mut permit: Permit, request: LoginRequest) -> Result<()> {
        permit.enter(Operation::Login)?;

        Arc::clone(&self.component).login(request).await?;

        let status =
            self.transition(StatusEvent::LoggedIn)
                .ok_or(OperationError::StatusChanged {
                    operation: Operation::Login,
                })?;

        self.init_if_ready(&mut permit, status).await
    }

    /// Reserves the component for an interactive logout.
    pub fn begin_logout(&self) -> Result<Permit> {
        self.reserve(Operation::Logout)
    }

    /// Completes an interactive logout.
    pub async fn logout(&self, mut permit: Permit) -> Result<()> {
        permit.enter(Operation::Logout)?;

        Arc::clone(&self.component).logout().await?;

        self.transition(StatusEvent::LoggedOut)
            .ok_or(OperationError::StatusChanged {
                operation: Operation::Logout,
            })?;
        Ok(())
    }

    async fn run_init(&self, permit: &mut Permit) -> Result<()> {
        permit.enter(Operation::Init)?;

        self.transition(StatusEvent::InitStarted)
            .ok_or_else(|| OperationError::NotAllowed {
                operation: Operation::Init,
                status: self.status(),
            })?;

        let result = Arc::clone(&self.component).init().await;
        self.transition(match &result {
            Ok(_) => StatusEvent::InitSucceeded,
            Err(e) => StatusEvent::InitFailed(e.clone()),
        });
        result?;

        if let Some(storefront) = self.storefront()
            && let Err(err) = storefront.sync_games(permit).await
        {
            warn!(component = self.id(), error = %err, "initial game sync failed");
        }
        Ok(())
    }

    async fn init_if_ready(&self, permit: &mut Permit, status: Status) -> Result<()> {
        if status.can_init() {
            self.run_init(permit).await
        } else {
            Ok(())
        }
    }

    fn reserve(&self, operation: Operation) -> Result<Permit> {
        let outcome = {
            let mut state = self.state_write();

            if !operation.allowed_by(&state.status) {
                Err(OperationError::NotAllowed {
                    operation,
                    status: state.status.clone(),
                })
            } else {
                state
                    .in_flight
                    .acquire(operation)
                    .map_err(|blocked_by| OperationError::Busy {
                        operation,
                        blocked_by,
                    })
            }
        };

        if let Err(err) = outcome {
            debug!(component = self.id(), %operation, reason = %err, "reserve rejected");
            return Err(err);
        }

        debug!(component = self.id(), %operation, "permit acquired");
        event::emit(Topic::Component);
        Ok(Permit::new(self.clone(), operation))
    }

    /// Applies `event` to the status, returning the new status. `None` if the
    /// transition is not valid.
    fn transition(&self, event: StatusEvent) -> Option<Status> {
        let event_debug = format!("{event:?}");

        let (status, previous) = {
            let mut state = self.state_write();
            match state.status.apply(event) {
                Some(next) => {
                    let previous = std::mem::replace(&mut state.status, next);
                    (state.status.clone(), Some(previous))
                }
                None => (state.status.clone(), None),
            }
        };

        let Some(previous) = previous else {
            warn!(
                component = self.id(),
                %status,
                event = event_debug,
                "ignoring invalid status transition"
            );
            return None;
        };

        if previous != status {
            if let Some(error) = status.error_message() {
                warn!(
                    component = self.id(),
                    from = %previous,
                    to = %status,
                    error,
                    event = event_debug,
                    "component entered error state"
                );
            } else {
                info!(
                    component = self.id(),
                    from = %previous,
                    to = %status,
                    "component status changed"
                );
            }
            event::emit(Topic::Component);
        }
        Some(status)
    }

    fn state_read(&self) -> RwLockReadGuard<'_, State> {
        self.state
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn state_write(&self) -> RwLockWriteGuard<'_, State> {
        self.state
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}
