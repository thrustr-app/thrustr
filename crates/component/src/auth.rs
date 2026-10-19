use crate::{
    ComponentHandle,
    handle::{OperationError, Permit, Result},
};
use domain::component::{Auth, AuthOperation, Form, LoginMethod, LoginRequest};
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

    /// Reserves the component for an interactive login.
    pub fn begin_login(&self) -> Result<LoginPermit> {
        let permit = Permit::begin(&self.component, AuthOperation::Login)?;
        Ok(LoginPermit {
            auth: Arc::clone(&self.auth),
            permit,
        })
    }

    pub async fn logout(&self) -> Result<()> {
        let permit = Permit::begin(&self.component, AuthOperation::Logout)?;
        let result = self.auth.logout().await;
        permit.finish(result.clone()).await?;
        Ok(result?)
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
            form.retain_valid(fields)?;
        }

        let result = self.auth.login(request).await;
        self.permit.finish(result.clone()).await?;
        Ok(result?)
    }
}
