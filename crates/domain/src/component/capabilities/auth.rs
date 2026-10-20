use crate::component::{
    Error, FieldValue, Form, FormElement, InvalidFormError, Operation, form::unique_ids,
};
use async_trait::async_trait;
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
    async fn login_flow(&self) -> Result<Option<LoginFlow>, Error>;
    async fn login(&self, request: LoginRequest) -> Result<(), Error>;
    async fn logout(&self) -> Result<(), Error>;
}

#[derive(Debug, Clone)]
pub enum LoginRequest {
    Flow { url: String, body: String },
    Form { fields: HashMap<String, FieldValue> },
}

#[derive(Debug, Clone)]
pub enum LoginMethod {
    Flow(LoginFlow),
    Form(LoginForm),
}

#[derive(Debug, Clone)]
pub struct LoginForm {
    elements: Vec<FormElement>,
}

impl LoginForm {
    pub fn new(elements: Vec<FormElement>) -> Result<Self, InvalidFormError> {
        unique_ids(elements.as_slice())?;
        let form = Self { elements };
        if let Some(field) = form.fields().find(|field| field.is_dynamic()) {
            return Err(InvalidFormError::DynamicFieldNotAllowed(
                field.id().to_owned(),
            ));
        }
        Ok(form)
    }
}

impl Form for LoginForm {
    fn elements(&self) -> impl Iterator<Item = &FormElement> {
        self.elements.iter()
    }
}

#[derive(Debug, Clone)]
pub struct LoginFlow {
    pub url: String,
    pub target: String,
}
