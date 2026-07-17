use crate::component::Field;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub enum LoginMethod {
    Flow(AuthFlow),
    Form(LoginForm),
}

#[derive(Deserialize, Debug, Clone)]
pub struct LoginForm {
    #[serde(rename = "field")]
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone)]
pub struct AuthFlow {
    pub url: String,
    pub target: String,
}
