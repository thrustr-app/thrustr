use crate::api::USER_BASIC;
use url::form_urlencoded;

pub fn auth_url() -> String {
    let redirect_url = format!(
        "https://www.epicgames.com/id/api/redirect?clientId={}&responseType=code",
        USER_BASIC
    );

    let encoded_redirect =
        form_urlencoded::byte_serialize(redirect_url.as_bytes()).collect::<String>();

    format!(
        "https://www.epicgames.com/id/login?redirectUrl={}",
        encoded_redirect
    )
}
