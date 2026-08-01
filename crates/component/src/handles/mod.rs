use self::error::Result;
use crate::RegistryContext;
use domain::component::{
    AuthFlow, Component, ComponentConfig, LoginMethod, LoginRequest, Metadata, Status, StatusEvent,
};
use event::Topic;
use std::{
    collections::HashMap,
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
};
use tracing::{debug, info, warn};

mod claim;
mod error;
mod storefront;

pub(crate) use claim::InFlight;
pub use claim::{Claim, Operation};
pub use error::OperationError;
pub use storefront::{StorefrontHandle, StorefrontOperation};

#[derive(Clone)]
pub struct ComponentHandle {
    component: Arc<dyn Component>,
    context: RegistryContext,
    status: Arc<RwLock<Status>>,
    in_flight: Arc<RwLock<InFlight>>,
}

impl ComponentHandle {
    pub fn new(component: Arc<dyn Component>, context: RegistryContext) -> Self {
        Self {
            component,
            context,
            status: Arc::new(RwLock::new(Status::Inactive)),
            in_flight: Arc::new(RwLock::new(InFlight::default())),
        }
    }

    pub fn id(&self) -> &str {
        self.component.metadata().id
    }

    pub fn metadata(&self) -> Metadata<'_> {
        self.component.metadata()
    }

    pub fn status(&self) -> Status {
        self.status
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
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

    /// The exclusive operation running on this component.
    pub fn running(&self) -> Option<Operation> {
        self.in_flight_read().exclusive()
    }

    /// Whether `operation` could be started right now.
    pub fn can(&self, operation: Operation) -> bool {
        operation.allowed_by(&self.status()) && self.in_flight_read().accepts(operation)
    }

    /// Claims the component for `operation`. `None` if the status forbids it or
    /// the component is busy with something incompatible.
    pub fn begin(&self, operation: Operation) -> Option<Claim> {
        let status = self.status();
        if !operation.allowed_by(&status) {
            debug!(component = self.id(), %operation, %status, "operation rejected");
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
        event::emit(Topic::Component);
        Some(Claim::new(self.clone(), operation))
    }

    pub async fn init(&self, claim: &mut Claim) -> Result<()> {
        self.enter(claim, Operation::Init)?;

        self.transition(StatusEvent::InitStarted)
            .ok_or_else(|| OperationError::NotAllowed {
                operation: Operation::Init,
                status: self.status(),
            })?;

        let result = self.component.init().await;
        self.transition(match &result {
            Ok(_) => StatusEvent::InitSucceeded,
            Err(e) => StatusEvent::InitFailed(e.clone()),
        });
        result?;

        // Game sync downgrades the claim to shared, so it no longer holds the
        // component exclusively for the rest of init.
        if let Some(storefront) = self.storefront()
            && let Err(err) = storefront.sync_games(claim).await
        {
            warn!(component = self.id(), error = %err, "initial game sync failed");
        }
        Ok(())
    }

    pub async fn login(&self, claim: &mut Claim, request: LoginRequest) -> Result<()> {
        self.enter(claim, Operation::Login)?;

        self.component.login(request).await?;

        let status =
            self.transition(StatusEvent::LoggedIn)
                .ok_or(OperationError::StatusChanged {
                    operation: Operation::Login,
                })?;

        if status.can_init() {
            return self.init(claim).await;
        }
        Ok(())
    }

    pub async fn logout(&self, claim: &mut Claim) -> Result<()> {
        self.enter(claim, Operation::Logout)?;

        self.component.logout().await?;

        self.transition(StatusEvent::LoggedOut)
            .ok_or(OperationError::StatusChanged {
                operation: Operation::Logout,
            })?;

        Ok(())
    }

    pub async fn save_config(
        &self,
        claim: &mut Claim,
        fields: HashMap<String, String>,
    ) -> Result<()> {
        self.enter(claim, Operation::Configure)?;

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

        if status.can_init() {
            return self.init(claim).await;
        }

        Ok(())
    }

    pub(super) fn enter(&self, claim: &mut Claim, operation: Operation) -> Result<()> {
        claim.transition(operation)?;

        let status = self.status();
        if !operation.allowed_by(&status) {
            debug!(component = self.id(), %operation, %status, "operation rejected");
            return Err(OperationError::NotAllowed { operation, status });
        }
        Ok(())
    }

    /// Applies `event` to the status, returning the new status. `None` if the
    /// transition is not valid.
    fn transition(&self, event: StatusEvent) -> Option<Status> {
        let event_debug = format!("{event:?}");

        let (status, previous) = {
            let mut guard = self
                .status
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            match guard.apply(event) {
                Some(next) => {
                    let previous = std::mem::replace(&mut *guard, next);
                    (guard.clone(), Some(previous))
                }
                None => (guard.clone(), None),
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
