use crate::RegistryContext;
use domain::component::{
    AuthFlow, Component, ComponentConfig, LoginMethod, LoginRequest, Metadata, Status, StatusEvent,
};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
};
use tracing::{debug, info, warn};

mod claim;
mod storefront;

pub(crate) use claim::InFlight;
pub use claim::{Claim, Operation};
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
        event::emit("component");
        Some(Claim::new(self.clone(), operation))
    }

    pub async fn init(&self, claim: &mut Claim) -> Result<(), String> {
        self.enter(claim, Operation::Init)?;

        self.transition(StatusEvent::InitStarted)
            .ok_or("Cannot initialize from current state")?;

        let result = self.component.init().await;
        self.transition(match &result {
            Ok(_) => StatusEvent::InitSucceeded,
            Err(e) => StatusEvent::InitFailed(e.clone()),
        });
        result.map_err(|e| e.to_string())?;

        // Game sync downgrades the claim to shared, so it no longer holds the
        // component exclusively for the rest of init.
        if let Some(storefront) = self.storefront()
            && let Err(err) = storefront.sync_games(claim).await
        {
            warn!(component = self.id(), error = %err, "initial game sync failed");
        }
        Ok(())
    }

    pub async fn login(&self, claim: &mut Claim, request: LoginRequest) -> Result<(), String> {
        self.enter(claim, Operation::Login)?;

        self.component
            .login(request)
            .await
            .map_err(|e| e.to_string())?;

        let status = self
            .transition(StatusEvent::LoggedIn)
            .ok_or("Logged in, but the component changed state meanwhile")?;

        if status.can_init() {
            return self.init(claim).await;
        }
        Ok(())
    }

    pub async fn logout(&self, claim: &mut Claim) -> Result<(), String> {
        self.enter(claim, Operation::Logout)?;

        self.component.logout().await.map_err(|e| e.to_string())?;

        self.transition(StatusEvent::LoggedOut)
            .ok_or("Logged out, but the component changed state meanwhile")?;

        Ok(())
    }

    pub async fn login_method(&self) -> Result<Option<LoginMethod>, String> {
        self.component
            .login_method()
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn logout_flow(&self) -> Result<Option<AuthFlow>, String> {
        self.component
            .logout_flow()
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn validate_config(&self, fields: HashMap<String, String>) -> Result<(), String> {
        self.component
            .validate_config(fields)
            .await
            .map_err(|e| e.to_string())
    }

    pub fn config_values(&self) -> Result<HashMap<String, String>, String> {
        self.context
            .component_storage
            .get_config_values(self.id())
            .map_err(|e| e.to_string())
    }

    pub async fn save_config(
        &self,
        claim: &mut Claim,
        fields: HashMap<String, String>,
    ) -> Result<(), String> {
        self.enter(claim, Operation::Configure)?;

        self.validate_config(fields.clone()).await?;
        self.context
            .component_storage
            .set_config_values(self.id(), &fields)
            .map_err(|e| {
                warn!(component = self.id(), error = %e, "storing configuration failed");
                e.to_string()
            })?;

        info!(component = self.id(), "configuration saved");

        let status = self
            .transition(StatusEvent::ConfigSaved)
            .ok_or("Configuration saved, but the component changed state meanwhile")?;

        if status.can_init() {
            return self.init(claim).await;
        }

        Ok(())
    }

    pub(super) fn enter(&self, claim: &mut Claim, operation: Operation) -> Result<(), String> {
        claim.transition(operation)?;

        let status = self.status();
        if !operation.allowed_by(&status) {
            debug!(component = self.id(), %operation, %status, "operation rejected");
            return Err(format!(
                "Cannot start {operation} while the component is {status}"
            ));
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
            event::emit("component");
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
