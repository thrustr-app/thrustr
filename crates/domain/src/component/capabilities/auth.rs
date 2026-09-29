use crate::component::{Error, Form, FormElement, Operation};
use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;
use strum::Display;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Display)]
#[strum(serialize_all = "lowercase")]
pub enum AuthOperation {
    Login,
    Logout,
}

impl From<AuthOperation> for Operation {
    fn from(operation: AuthOperation) -> Self {
        Self::Auth(operation)
    }
}

#[async_trait]
pub trait Auth: Send + Sync {
    fn login_form(&self) -> Option<&LoginForm>;
    async fn login_flow(&self) -> Result<Option<AuthFlow>, Error>;
    async fn logout_flow(&self) -> Result<Option<AuthFlow>, Error>;
    async fn login(&self, request: LoginRequest) -> Result<(), Error>;
    async fn logout(&self) -> Result<(), Error>;
}

#[derive(Debug, Clone)]
pub enum LoginRequest {
    Flow { url: String, body: String },
    Form { fields: HashMap<String, String> },
}

#[derive(Debug, Clone)]
pub enum LoginMethod {
    Flow(AuthFlow),
    Form(LoginForm),
}

#[derive(Deserialize, Debug, Clone)]
pub struct LoginForm {
    pub elements: Vec<FormElement>,
}

impl Form for LoginForm {
    fn elements(&self) -> impl Iterator<Item = &FormElement> {
        self.elements.iter()
    }
}

#[derive(Debug, Clone)]
pub struct AuthFlow {
    pub url: String,
    pub target: String,
}
