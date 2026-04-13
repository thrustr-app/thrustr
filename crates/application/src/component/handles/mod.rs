use crate::component::RegistryContext;
use domain::component::{
    AuthFlow, Component, ComponentConfig, ComponentStatus, LoginMethod, Metadata,
};
use std::sync::{Arc, RwLock};

mod storefront;

pub use storefront::StorefrontHandle;

#[derive(Clone)]
pub struct ComponentHandle {
    component: Arc<dyn Component>,
    context: RegistryContext,
    status: Arc<RwLock<ComponentStatus>>,
}

impl ComponentHandle {
    pub fn new(component: Arc<dyn Component>, context: RegistryContext) -> Self {
        Self {
            component,
            context,
            status: Arc::new(RwLock::new(ComponentStatus::Inactive)),
        }
    }

    pub fn id(&self) -> &str {
        self.component.metadata().id.as_str()
    }

    pub fn metadata(&self) -> &Metadata {
        self.component.metadata()
    }

    pub fn status(&self) -> ComponentStatus {
        self.status.read().unwrap().clone()
    }

    pub fn config(&self) -> Option<&ComponentConfig> {
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
        self.set_status(ComponentStatus::Initializing);
        let result = self.component.init().await;
        self.set_status(match &result {
            Ok(_) => ComponentStatus::Active,
            Err(e) => ComponentStatus::InitError(e.clone()),
        });
        result.map_err(|e| e.to_string())?;

        // TODO: temporal
        self.storefront().unwrap().fetch_games().await.unwrap();
        Ok(())
    }

    pub async fn login(
        &self,
        url: Option<String>,
        body: Option<String>,
        fields: Option<Vec<(String, String)>>,
    ) -> Result<(), String> {
        if !self.status().can_login() {
            return Err("Cannot login from current state".into());
        }
        let prior = self.status();
        let result = self.component.login(url, body, fields).await;
        if result.is_ok() {
            self.set_status(match prior {
                ComponentStatus::Unauthenticated => ComponentStatus::Active,
                _ => ComponentStatus::Inactive,
            });
            if self.status().can_init() {
                return self.init().await;
            }
        }
        result.map_err(|e| e.to_string())
    }

    pub async fn logout(&self) -> Result<(), String> {
        if !self.status().can_logout() {
            return Err("Cannot logout from current state".into());
        }
        let result = self.component.logout().await;
        if result.is_ok() {
            self.set_status(ComponentStatus::Unauthenticated);
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

    pub async fn validate_config(&self, fields: &[(String, String)]) -> Result<(), String> {
        self.component
            .validate_config(fields)
            .await
            .map_err(|e| e.to_string())
    }

    pub fn get_config_values(&self) -> Vec<(String, String)> {
        self.context
            .component_storage
            .get_config_values(self.id())
            .unwrap()
    }

    pub async fn save_config(&self, fields: &[(String, String)]) -> Result<(), String> {
        if !self.status().can_configure() {
            return Err("Cannot configure from current state".into());
        }
        self.validate_config(fields).await?;
        self.context
            .component_storage
            .set_config_values(self.id(), fields)
            .map_err(|e| e.to_string())?;

        if self.status().can_init() {
            return self.init().await;
        }

        Ok(())
    }

    fn set_status(&self, status: ComponentStatus) {
        *self.status.write().unwrap() = status;
        event::emit("component");
    }
}
