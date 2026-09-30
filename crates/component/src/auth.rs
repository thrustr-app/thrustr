use crate::{
    ComponentHandle,
    handle::{OperationError, Permit, Result},
};
use domain::component::{Auth, AuthFlow, AuthOperation, Form, LoginMethod, LoginRequest};
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
        if let Some(flow) = self.auth.login_flow().await? {
            return Ok(LoginMethod::Flow(flow));
        }

        let form = self
            .auth
            .login_form()
            .ok_or(OperationError::NoLoginMethod)?;
        Ok(LoginMethod::Form(form.clone()))
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
    pub async fn login(self, mut request: LoginRequest) -> Result<()> {
        if let LoginRequest::Form { fields } = &mut request {
            let form = self.auth.login_form().ok_or(OperationError::NoLoginForm)?;
            form.check(fields)?;
        }

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
