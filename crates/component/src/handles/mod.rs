use self::error::Result;
use crate::RegistryContext;
use domain::component::{
    Auth, Capabilities, Component, Config, Error, Initialization, Metadata, Operation, Outcome,
    Running, State, Status, Storefront,
};
use event::Topic;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
use tracing::{debug, info, warn};

mod auth;
mod config;
mod error;
mod permit;
mod storefront;

pub use auth::{AuthHandle, LoginPermit, LogoutPermit};
pub use config::ConfigHandle;
pub use error::OperationError;
pub use storefront::StorefrontHandle;

#[derive(Clone)]
pub struct ComponentHandle {
    component: Arc<dyn Component>,
    auth: Option<Arc<dyn Auth>>,
    config: Option<Arc<dyn Config>>,
    storefront: Option<Arc<dyn Storefront>>,
    context: RegistryContext,
    state: Arc<RwLock<State>>,
}

impl ComponentHandle {
    pub fn new(component: Arc<dyn Component>, context: RegistryContext) -> Self {
        let auth = component.auth();
        let config = component.config();
        let storefront = component.storefront();
        let capabilities = Capabilities {
            auth: auth.is_some(),
            config: config.is_some(),
            storefront: storefront.is_some(),
        };

        Self {
            component,
            auth,
            config,
            storefront,
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

    pub fn auth(&self) -> Option<AuthHandle> {
        self.auth
            .clone()
            .map(|auth| AuthHandle::new(auth, self.clone()))
    }

    pub fn config(&self) -> Option<ConfigHandle> {
        self.config
            .clone()
            .map(|config| ConfigHandle::new(config, self.clone()))
    }

    pub fn storefront(&self) -> Option<StorefrontHandle> {
        self.storefront
            .clone()
            .map(|storefront| StorefrontHandle::new(storefront, self.clone()))
    }

    /// Whether the component supports `operation` and its status permits it,
    /// ignoring what is already running.
    pub fn allows(&self, operation: impl Into<Operation>) -> bool {
        self.state_read().allows(operation.into())
    }

    /// Whether `operation` could be started right now.
    pub fn can(&self, operation: impl Into<Operation>) -> bool {
        self.state_read().can(operation.into())
    }

    /// Whether a failed component could be reinitialized right now.
    pub fn can_reinitialize(&self) -> bool {
        self.state_read().can_reinitialize()
    }

    pub fn is_running(&self, operation: impl Into<Operation>) -> bool {
        self.state_read().is_running(operation.into())
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
