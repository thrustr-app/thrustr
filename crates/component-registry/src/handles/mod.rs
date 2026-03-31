use application::{
    component::{AuthFlow, Component, ComponentStorage, Config, LoginMethod, Metadata, Status},
    domain::game::GameRepository,
};
use std::sync::Arc;
pub use storefront::StorefrontHandle;

mod storefront;

#[derive(Clone)]
pub struct ComponentHandle {
    component: Arc<dyn Component>,
    component_storage: Arc<dyn ComponentStorage>,
    game_storage: Arc<dyn GameRepository>,
}

impl ComponentHandle {
    pub fn new(
        component: Arc<dyn Component>,
        component_storage: Arc<dyn ComponentStorage>,
        game_storage: Arc<dyn GameRepository>,
    ) -> Self {
        Self {
            component,
            component_storage,
            game_storage,
        }
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

    pub fn storefront(&self) -> Option<StorefrontHandle> {
        Arc::clone(&self.component).storefront().map(|storefront| {
            StorefrontHandle::new(
                storefront,
                Arc::clone(&self.component_storage),
                Arc::clone(&self.game_storage),
            )
        })
    }

    pub async fn init(&self) -> Result<(), String> {
        if !self.component.status().can_init() {
            return Err("Cannot initialize from current state".into());
        }
        self.set_status(Status::Initializing);
        let result = self.component.init().await;
        self.set_status(match &result {
            Ok(_) => Status::Active,
            Err(e) => Status::InitError(e.clone()),
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
        if !self.component.status().can_login() {
            return Err("Cannot login from current state".into());
        }
        let prior = self.component.status();
        let result = self.component.login(url, body, fields).await;
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

    pub async fn logout(&self) -> Result<(), String> {
        if !self.component.status().can_logout() {
            return Err("Cannot logout from current state".into());
        }
        let result = self.component.logout().await;
        if result.is_ok() {
            self.set_status(Status::Unauthenticated);
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
        self.component_storage.get_config_values(self.id()).unwrap()
    }

    pub async fn save_config(&self, fields: &[(String, String)]) -> Result<(), String> {
        if !self.component.status().can_configure() {
            return Err("Cannot configure from current state".into());
        }
        self.validate_config(fields).await?;
        self.component_storage
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
