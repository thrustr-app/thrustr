use crate::{
    AuthHandle, ConfigHandle, RegistryContext, StorefrontHandle, Timers, timers::ComponentTimers,
};
use domain::component::{
    Activation, Auth, Capabilities, Component, Config, Error, Initialization, Metadata,
    MissingFieldError, Operation, Outcome, Rejection, Running, State, Status, Storefront,
};
use event::Topic;
use std::sync::{Arc, OnceLock, RwLock, RwLockReadGuard, RwLockWriteGuard, Weak};
use tracing::{debug, info, warn};

#[derive(Clone)]
pub struct ComponentHandle(Arc<Inner>);

struct Inner {
    component: Arc<dyn Component>,
    auth: Option<Arc<dyn Auth>>,
    config: Option<Arc<dyn Config>>,
    storefront: Option<Arc<dyn Storefront>>,
    timers: Option<ComponentTimers>,
    context: RegistryContext,
    state: RwLock<State>,
}

impl ComponentHandle {
    pub(crate) fn new(component: Arc<dyn Component>, context: RegistryContext) -> Self {
        let auth = component.auth();
        let config = component.config();
        let storefront = component.storefront();
        let scheduler = component.scheduler();
        let capabilities = Capabilities {
            auth: auth.is_some(),
            config: config.is_some(),
            storefront: storefront.is_some(),
            scheduler: scheduler.is_some(),
        };

        Self(Arc::new(Inner {
            component,
            auth,
            config,
            storefront,
            timers: scheduler
                .map(|scheduler| ComponentTimers::new(scheduler, context.tokio_handle.clone())),
            context,
            state: RwLock::new(State::new(capabilities)),
        }))
    }

    pub(crate) fn downgrade(&self) -> WeakComponentHandle {
        WeakComponentHandle(Arc::downgrade(&self.0))
    }

    pub fn id(&self) -> &str {
        self.0.component.metadata().id
    }

    pub fn metadata(&self) -> Metadata<'_> {
        self.0.component.metadata()
    }

    pub fn status(&self) -> Status {
        self.state_read().status().clone()
    }

    /// The current activation and whether the component is already active, or
    /// the status if the component is neither initializing nor active.
    pub(crate) fn activation(&self) -> std::result::Result<(Activation, bool), Status> {
        let state = self.state_read();
        let status = state.status();
        state
            .activation()
            .map(|activation| (activation, status.is_active()))
            .ok_or_else(|| status.clone())
    }

    pub fn auth(&self) -> Option<AuthHandle> {
        self.0
            .auth
            .clone()
            .map(|auth| AuthHandle::new(auth, self.clone()))
    }

    pub fn config(&self) -> Option<ConfigHandle> {
        self.0
            .config
            .clone()
            .map(|config| ConfigHandle::new(config, self.clone()))
    }

    pub fn storefront(&self) -> Option<StorefrontHandle> {
        self.0
            .storefront
            .clone()
            .map(|storefront| StorefrontHandle::new(storefront, self.clone()))
    }

    pub(crate) fn timers(&self) -> Option<&ComponentTimers> {
        self.0.timers.as_ref()
    }

    pub(crate) fn context(&self) -> &RegistryContext {
        &self.0.context
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
        let result = self.0.component.init().await;
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

    fn start(&self, operation: Operation, activation: Option<Activation>) -> Result<Running> {
        let started = match activation {
            Some(activation) => self.state_write().start_in(activation, operation),
            None => self.state_write().start(operation),
        };
        let running = started.map_err(|rejection| {
            debug!(component = self.id(), %operation, %rejection, "operation rejected");
            OperationError::Rejected {
                operation,
                rejection,
            }
        })?;

        debug!(component = self.id(), %operation, "operation started");
        event::emit(Topic::ComponentState);
        Ok(running)
    }

    fn cancel(&self, running: Running) {
        let operation = running.operation();
        self.state_write().cancel(running);

        debug!(component = self.id(), %operation, "operation cancelled");
        event::emit(Topic::ComponentState);
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

            if let Some(timers) = self.timers() {
                match to.is_active() {
                    true => timers.activate(self),
                    false => timers.prune(self),
                }
            }
        }
        if let Some(error) = &outcome.transient_error {
            warn!(component = self.id(), %error, "operation failed");
        }

        event::emit(Topic::ComponentState);
    }

    fn state_read(&self) -> RwLockReadGuard<'_, State> {
        self.0
            .state
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn state_write(&self) -> RwLockWriteGuard<'_, State> {
        self.0
            .state
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// A [`ComponentHandle`] that does not keep the component alive.
#[derive(Debug, Clone)]
pub(crate) struct WeakComponentHandle(Weak<Inner>);

impl WeakComponentHandle {
    pub(crate) fn upgrade(&self) -> Option<ComponentHandle> {
        self.0.upgrade().map(ComponentHandle)
    }
}

/// A weak reference to a component's handle.
///
/// Lets a component act on itself through the host (e.g. schedule timers
/// tied to its current activation and status).
#[derive(Clone)]
pub struct ComponentLink(Arc<OnceLock<WeakComponentHandle>>);

impl ComponentLink {
    pub(crate) fn new() -> Self {
        Self(Arc::default())
    }

    pub fn timers(&self) -> Timers<'_> {
        Timers::new(self)
    }

    pub(crate) fn bind(&self, handle: &ComponentHandle) {
        self.0
            .set(handle.downgrade())
            .expect("link should be bound by a single registration");
    }

    pub(crate) fn handle(&self) -> Option<ComponentHandle> {
        self.0.get()?.upgrade()
    }
}

/// Holds a component while an operation is running.
///
/// Component errors are reported through [`Permit::finish`], as they can
/// change the status. Any other failure drops the permit, which cancels the
/// operation.
pub(crate) struct Permit {
    handle: ComponentHandle,
    running: Option<Running>,
}

impl Permit {
    pub(crate) fn begin(handle: &ComponentHandle, operation: impl Into<Operation>) -> Result<Self> {
        let running = handle.start(operation.into(), None)?;
        Ok(Self::new(handle, running))
    }

    /// Like [`Permit::begin`], but only while `activation` is still current.
    pub(crate) fn begin_in(
        handle: &ComponentHandle,
        activation: Activation,
        operation: impl Into<Operation>,
    ) -> Result<Self> {
        let running = handle.start(operation.into(), Some(activation))?;
        Ok(Self::new(handle, running))
    }

    fn new(handle: &ComponentHandle, running: Running) -> Self {
        Self {
            handle: handle.clone(),
            running: Some(running),
        }
    }

    /// Reports the result of the operation, running any initialization it
    /// starts.
    pub(crate) async fn finish(mut self, result: std::result::Result<(), Error>) -> Result<()> {
        let running = self
            .running
            .take()
            .expect("permit should hold its operation until finished");
        let outcome = self.handle.state_write().finish(running, result);
        self.handle.report(&outcome);
        self.handle.initialize(outcome.initialization).await
    }
}

impl Drop for Permit {
    fn drop(&mut self) {
        if let Some(running) = self.running.take() {
            self.handle.cancel(running);
        }
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

pub(crate) type Result<T> = std::result::Result<T, OperationError>;

#[derive(Debug, thiserror::Error)]
pub enum OperationError {
    #[error("cannot start {operation}: {rejection}")]
    Rejected {
        operation: Operation,
        rejection: Rejection,
    },

    #[error("cannot initialize: {0}")]
    NotInitializable(Rejection),

    #[error(transparent)]
    MissingField(#[from] MissingFieldError),

    #[error("component has no login form")]
    NoLoginForm,

    #[error("component offers neither a login flow nor a login form")]
    NoLoginMethod,

    #[error(transparent)]
    Component(#[from] Error),

    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}
