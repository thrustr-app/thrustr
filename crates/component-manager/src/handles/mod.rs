use ports::{
    capabilities::Storefront,
    component::{AuthFlow, Component, Config, Metadata, Status},
    storage::ComponentStorage,
};
use std::sync::Arc;

mod storefront;

pub use storefront::StorefrontHandle;

#[derive(Clone)]
pub struct ComponentHandle {
    component: Arc<dyn Component>,
    storage: Arc<dyn ComponentStorage>,
}

impl ComponentHandle {
    pub fn new(component: Arc<dyn Component>, storage: Arc<dyn ComponentStorage>) -> Self {
        Self { component, storage }
    }

    pub fn id(&self) -> &str {
        self.component.metadata().id.as_str()
    }

    pub fn metadata(&self) -> &Metadata {
        self.component.metadata()
    }

    pub fn status(&self) -> Status {
        self.component.status()
    }

    pub fn config(&self) -> Option<&Config> {
        self.component.config()
    }

    pub fn storefront(&self) -> Option<Arc<dyn Storefront>> {
        Arc::clone(&self.component).storefront()
    }

    pub async fn init(&self) -> Result<(), String> {
        if !self.component.status().can_init() {
            return Err("Component must be Inactive or in InitError state to initialize".into());
        }
        self.set_status(Status::Initializing);
        let result = self.component.init().await;
        self.set_status(match &result {
            Ok(_) => Status::Active,
            Err(e) => Status::InitError(e.clone()),
        });
        result.map_err(|e| e.to_string())
    }

    pub async fn login(&self, url: String, body: String) -> Result<(), String> {
        let prior = self.component.status();
        let result = self.component.login(url, body).await;
        if result.is_ok() {
            self.set_status(match prior {
                Status::Unauthenticated => Status::Active,
                _ => Status::Inactive,
            });
            if self.component.status().can_init() {
                return self.init().await;
            }
        }
        result.map_err(|e| e.to_string())
    }

    pub async fn logout(&self, url: String, body: String) -> Result<(), String> {
        let result = self.component.logout(url, body).await;
        if result.is_ok() {
            self.set_status(Status::Unauthenticated);
        }
        result.map_err(|e| e.to_string())
    }

    pub async fn get_login_flow(&self) -> Result<Option<AuthFlow>, String> {
        self.component
            .get_login_flow()
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
        self.storage.get_config_values(self.id()).unwrap()
    }

    pub async fn save_config(&self, fields: &[(String, String)]) -> Result<(), String> {
        self.validate_config(fields).await.unwrap();
        self.storage
            .set_config_values(self.id(), fields)
            .map_err(|e| e.to_string())?;

        if self.component.status().can_init() {
            return self.init().await;
        }

        Ok(())
    }

    fn set_status(&self, status: Status) {
        self.component.set_status(status);
        event::emit("component");
    }
}
