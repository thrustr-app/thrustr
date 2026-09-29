use crate::component::{
    Error, FormElement, MissingFieldError, Operation, TextField, form, text_fields,
};
use async_trait::async_trait;
use serde::Deserialize;
use std::{borrow::Borrow, collections::HashMap, hash::Hash};
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
    async fn login_method(&self) -> Result<LoginMethod, Error>;
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
    #[serde(rename = "field")]
    pub elements: Vec<FormElement>,
}

impl LoginForm {
    pub fn text_fields(&self) -> impl Iterator<Item = &TextField> {
        text_fields(&self.elements)
    }

    /// Checks every required field has a value and drops values for
    /// undeclared fields.
    pub fn check(&self, values: &mut HashMap<String, String>) -> Result<(), MissingFieldError> {
        form::check(|| self.text_fields(), values)
    }

    /// Returns the first required field without a value.
    pub fn missing_field<K, V>(&self, values: &HashMap<K, V>) -> Option<&TextField>
    where
        K: Borrow<str> + Hash + Eq,
        V: AsRef<str>,
    {
        form::missing_field(self.text_fields(), values)
    }
}

#[derive(Debug, Clone)]
pub struct AuthFlow {
    pub url: String,
    pub target: String,
}
