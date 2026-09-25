use crate::component::{FormElement, TextField, text_fields};
use serde::Deserialize;
use std::collections::HashMap;

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
}

#[derive(Debug, Clone)]
pub struct AuthFlow {
    pub url: String,
    pub target: String,
}
