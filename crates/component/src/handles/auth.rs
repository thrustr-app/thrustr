use super::error::Result;
use super::permit::Permit;
use crate::ComponentHandle;
use domain::component::{Auth, AuthFlow, AuthOperation, LoginMethod, LoginRequest};
use std::sync::Arc;

#[derive(Clone)]
pub struct AuthHandle {
    auth: Arc<dyn Auth>,
    component: ComponentHandle,
}

impl AuthHandle {
    pub(crate) fn new(auth: Arc<dyn Auth>, component: ComponentHandle) -> Self {
        Self { auth, component }
    }

    pub fn component(&self) -> &ComponentHandle {
        &self.component
    }

    pub async fn login_method(&self) -> Result<LoginMethod> {
        Ok(self.auth.login_method().await?)
    }

    pub async fn logout_flow(&self) -> Result<Option<AuthFlow>> {
        Ok(self.auth.logout_flow().await?)
    }

    /// Reserves the component for an interactive login.
    pub fn begin_login(&self) -> Result<LoginPermit> {
        let permit = Permit::begin(&self.component, AuthOperation::Login)?;
        Ok(LoginPermit {
            auth: Arc::clone(&self.auth),
            permit,
        })
    }

    /// Reserves the component for an interactive logout.
    pub fn begin_logout(&self) -> Result<LogoutPermit> {
        let permit = Permit::begin(&self.component, AuthOperation::Logout)?;
        Ok(LogoutPermit {
            auth: Arc::clone(&self.auth),
            permit,
        })
    }
}

pub struct LoginPermit {
    auth: Arc<dyn Auth>,
    permit: Permit,
}

impl LoginPermit {
    pub async fn login(self, request: LoginRequest) -> Result<()> {
        let result = self.auth.login(request).await;
        self.permit.finish(result.clone()).await?;
        Ok(result?)
    }
}

pub struct LogoutPermit {
    auth: Arc<dyn Auth>,
    permit: Permit,
}

impl LogoutPermit {
    pub async fn logout(self) -> Result<()> {
        let result = self.auth.logout().await;
        self.permit.finish(result.clone()).await?;
        Ok(result?)
    }
}
