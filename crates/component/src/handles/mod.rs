use self::error::Result;
use crate::RegistryContext;
use domain::component::{
    AuthFlow, Capabilities, Component, ComponentConfig, Error, Initialization, LoginMethod,
    LoginRequest, Metadata, Operation, Outcome, Running, State, Status,
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
use permit::Permit;
pub use permit::{LoginPermit, LogoutPermit};
pub use storefront::StorefrontHandle;

#[derive(Clone)]
pub struct ComponentHandle {
    component: Arc<dyn Component>,
    context: RegistryContext,
    state: Arc<RwLock<State>>,
}

impl ComponentHandle {
    pub fn new(
        component: Arc<dyn Component>,
        context: RegistryContext,
        capabilities: Capabilities,
    ) -> Self {
        Self {
            component,
            context,
            state: Arc::new(RwLock::new(State::new(capabilities))),
        }
    }

    pub fn id(&self) -> &str {
        self.component.metadata().id
    }

    pub fn metadata(&self) -> Metadata<'_> {
        self.component.metadata()
    }

    pub fn status(&self) -> Status {
        self.state_read().status().clone()
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

    pub fn config_values(&self) -> Result<HashMap<String, String>> {
        Ok(self
            .context
            .component_storage
            .get_config_values(self.id())?)
    }

    /// Whether the component supports `operation` and its status permits it,
    /// ignoring what is already running.
    pub fn allows(&self, operation: Operation) -> bool {
        self.state_read().allows(operation)
    }

    /// Whether `operation` could be started right now.
    pub fn can(&self, operation: Operation) -> bool {
        self.state_read().can(operation)
    }

    /// Whether a failed component could be reinitialized right now.
    pub fn can_reinitialize(&self) -> bool {
        self.state_read().can_reinitialize()
    }

    pub fn is_running(&self, operation: Operation) -> bool {
        self.state_read().is_running(operation)
    }

    pub async fn enable(&self) -> Result<()> {
        let outcome = self
            .state_write()
            .enable()
            .map_err(OperationError::NotInitializable)?;
        self.report(&outcome);
        self.initialize(outcome.initialization).await
    }

    pub async fn reinitialize(&self) -> Result<()> {
        let outcome = self
            .state_write()
            .reinitialize()
            .map_err(OperationError::NotInitializable)?;
        self.report(&outcome);
        self.initialize(outcome.initialization).await
    }

    pub async fn save_config(&self, fields: HashMap<String, String>) -> Result<()> {
        let permit = Permit::begin(self, Operation::Config)?;

        if let Err(error) = self.component.validate_config(fields.clone()).await {
            permit.finish(Err(error.clone())).await?;
            return Err(error.into());
        }
        self.context
            .component_storage
            .set_config_values(self.id(), &fields)
            .map_err(|e| {
                warn!(component = self.id(), error = %e, "storing configuration failed");
                e
            })?;

        info!(component = self.id(), "configuration saved");
        permit.finish(Ok(())).await
    }

    /// Reserves the component for an interactive login.
    pub fn begin_login(&self) -> Result<LoginPermit> {
        Permit::begin(self, Operation::Login).map(LoginPermit)
    }

    /// Completes an interactive login and initializes the component if needed.
    pub async fn login(
        &self,
        LoginPermit(permit): LoginPermit,
        request: LoginRequest,
    ) -> Result<()> {
        let result = Arc::clone(&self.component).login(request).await;
        permit.finish(result.clone()).await?;
        Ok(result?)
    }

    /// Reserves the component for an interactive logout.
    pub fn begin_logout(&self) -> Result<LogoutPermit> {
        Permit::begin(self, Operation::Logout).map(LogoutPermit)
    }

    /// Completes an interactive logout.
    pub async fn logout(&self, LogoutPermit(permit): LogoutPermit) -> Result<()> {
        let result = Arc::clone(&self.component).logout().await;
        permit.finish(result.clone()).await?;
        Ok(result?)
    }

    async fn initialize(&self, initialization: Option<Initialization>) -> Result<()> {
        let Some(initialization) = initialization else {
            return Ok(());
        };

        let initializing = Initializing {
            handle: self,
            initialization: Some(initialization),
        };
        let result = self.component.init().await;
        initializing.finish(result.clone());
        result?;

        // Boxed because syncing finishes through `Permit::finish`, which awaits
        // `initialize`. A sync never starts an initialization but the compiler
        // can't know that.
        if let Some(storefront) = self.storefront()
            && let Err(err) = Box::pin(storefront.sync_games()).await
        {
            warn!(component = self.id(), error = %err, "initial game sync failed");
        }
        Ok(())
    }

    fn start(&self, operation: Operation) -> Result<Running> {
        let started = self.state_write().start(operation);
        let running = started.map_err(|rejection| {
            debug!(component = self.id(), %operation, %rejection, "operation rejected");
            OperationError::Rejected {
                operation,
                rejection,
            }
        })?;

        debug!(component = self.id(), %operation, "operation started");
        event::emit(Topic::Component);
        Ok(running)
    }

    fn cancel(&self, running: Running) {
        let operation = running.operation();
        self.state_write().cancel(running);

        debug!(component = self.id(), %operation, "operation cancelled");
        event::emit(Topic::Component);
    }

    fn finish_initialization(
        &self,
        initialization: Initialization,
        result: std::result::Result<(), Error>,
    ) {
        let outcome = self
            .state_write()
            .finish_initialization(initialization, result);
        self.report(&outcome);
    }

    fn report(&self, outcome: &Outcome) {
        if let Some((from, to)) = &outcome.change {
            match to.error() {
                Some(error) => warn!(
                    component = self.id(),
                    %from,
                    %to,
                    %error,
                    "component entered error state"
                ),
                None => info!(
                    component = self.id(),
                    %from,
                    %to,
                    "component status changed"
                ),
            }
        }
        if let Some(error) = &outcome.transient_error {
            warn!(component = self.id(), %error, "operation failed");
        }

        event::emit(Topic::Component);
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

/// Reports a running initialization as failed if dropped before it finishes,
/// so the component is never left initializing.
struct Initializing<'a> {
    handle: &'a ComponentHandle,
    initialization: Option<Initialization>,
}

impl Initializing<'_> {
    fn finish(mut self, result: std::result::Result<(), Error>) {
        let initialization = self
            .initialization
            .take()
            .expect("initialization should be pending until finished");
        self.handle.finish_initialization(initialization, result);
    }
}

impl Drop for Initializing<'_> {
    fn drop(&mut self) {
        if let Some(initialization) = self.initialization.take() {
            let error = Error::Other("initialization cancelled".into());
            self.handle
                .finish_initialization(initialization, Err(error));
        }
    }
}
