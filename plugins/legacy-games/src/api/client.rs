use crate::api::{
    USER_TOKEN_HEADER, endpoints,
    error::Error,
    models::{IsExistsByEmailResponse, LoginResponse},
};
use golem_wasi_http::{
    Client,
    header::{ACCEPT, AUTHORIZATION, CACHE_CONTROL, CONTENT_TYPE},
};

pub fn giveaway_login(email: &str) -> Result<IsExistsByEmailResponse, Error> {
    Ok(Client::new()
        .get(endpoints::is_exists_by_email(email))
        .header(AUTHORIZATION, "?token?")
        .header(ACCEPT, "application/json")
        .header(CONTENT_TYPE, "application/json")
        .header(CACHE_CONTROL, "no-cache")
        .send()?
        .json()?)
}

pub fn wp_login(token: &str) -> Result<LoginResponse, Error> {
    let user_token = format!("Basic {token}");
    Ok(Client::new()
        .get(endpoints::login())
        .header(USER_TOKEN_HEADER, user_token)
        .header(AUTHORIZATION, "?token?")
        .header(ACCEPT, "application/json")
        .header(CONTENT_TYPE, "application/json")
        .header(CACHE_CONTROL, "no-cache")
        .send()?
        .json()?)
}
