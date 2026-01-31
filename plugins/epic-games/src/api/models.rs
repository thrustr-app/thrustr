use serde::Deserialize;

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AuthResponse {
    pub authorization_code: String,
}
