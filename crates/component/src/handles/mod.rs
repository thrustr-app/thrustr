use crate::RegistryContext;
use domain::component::{
    AuthFlow, Component, ComponentConfig, LoginMethod, LoginRequest, Metadata, Status, StatusEvent,
};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};
use tracing::warn;

mod storefront;

pub use storefront::StorefrontHandle;

#[derive(Clone)]
pub struct ComponentHandle {
    component: Arc<dyn Component>,
    context: RegistryContext,
    status: Arc<RwLock<Status>>,
}

impl ComponentHandle {
    pub fn new(component: Arc<dyn Component>, context: RegistryContext) -> Self {
        Self {
            component,
            context,
            status: Arc::new(RwLock::new(Status::Inactive)),
        }
    }

    pub fn id(&self) -> &str {
        self.component.metadata().id
    }

    pub fn metadata(&self) -> Metadata<'_> {
        self.component.metadata()
    }

    pub fn status(&self) -> Status {
        self.status.read().unwrap().clone()
    }

    pub fn config(&self) -> Option<ComponentConfig> {
        self.component.config()
    }

    pub fn storefront(&self) -> Option<StorefrontHandle> {
        Arc::clone(&self.component)
            .storefront()
            .map(|storefront| StorefrontHandle::new(storefront, self.clone()))
    }

    pub async fn init(&self) -> Result<(), String> {
        if !self.status().can_init() {
            return Err("Cannot initialize from current state".into());
        }
        self.transition(StatusEvent::InitStarted);
        let result = self.component.init().await;
        self.transition(match &result {
            Ok(_) => StatusEvent::InitSucceeded,
            Err(e) => StatusEvent::InitFailed(e.clone()),
        });
        result.map_err(|e| e.to_string())?;

        if let Some(storefront) = self.storefront()
            && let Err(err) = storefront.sync_games().await
        {
            warn!(component = self.id(), "initial game sync failed: {err}");
        }
        Ok(())
    }

    pub async fn login(&self, request: LoginRequest) -> Result<(), String> {
        if !self.status().can_login() {
            return Err("Cannot login from current state".into());
        }
        let result = self.component.login(request).await;
        if result.is_ok() && self.transition(StatusEvent::LoggedIn).can_init() {
            return self.init().await;
        }
        result.map_err(|e| e.to_string())
    }

    pub async fn logout(&self) -> Result<(), String> {
        if !self.status().can_logout() {
            return Err("Cannot logout from current state".into());
        }
        let result = self.component.logout().await;
        if result.is_ok() {
            self.transition(StatusEvent::LoggedOut);
        }
        result.map_err(|e| e.to_string())
    }

    pub async fn get_login_method(&self) -> Result<Option<LoginMethod>, String> {
        self.component
            .get_login_method()
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn get_logout_flow(&self) -> Result<Option<AuthFlow>, String> {
        self.component
            .get_logout_flow()
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn validate_config(&self, fields: HashMap<String, String>) -> Result<(), String> {
        self.component
            .validate_config(fields)
            .await
            .map_err(|e| e.to_string())
    }

    pub fn get_config_values(&self) -> Result<HashMap<String, String>, String> {
        self.context
            .component_storage
            .get_config_values(self.id())
            .map_err(|e| e.to_string())
    }

    pub async fn save_config(&self, fields: HashMap<String, String>) -> Result<(), String> {
        if !self.status().can_configure() {
            return Err("Cannot configure from current state".into());
        }
        self.validate_config(fields.clone()).await?;
        self.context
            .component_storage
            .set_config_values(self.id(), &fields)
            .map_err(|e| e.to_string())?;

        if self.transition(StatusEvent::ConfigSaved).can_init() {
            return self.init().await;
        }

        Ok(())
    }

    fn transition(&self, event: StatusEvent) -> Status {
        let (status, changed) = {
            let mut guard = self.status.write().unwrap();
            let event_debug = format!("{event:?}");
            match guard.apply(event) {
                Some(next) => {
                    let changed = next != *guard;
                    *guard = next;
                    (guard.clone(), changed)
                }
                None => {
                    warn!(
                        component = self.id(),
                        status = ?*guard,
                        event = event_debug,
                        "ignoring invalid status transition"
                    );
                    (guard.clone(), false)
                }
            }
        };
        if changed {
            event::emit("component");
        }
        status
    }
}
