use crate::api;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use pdk::{Error, LoginFlow, cache::Cache};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use url::{Url, form_urlencoded};

const AUTHORIZE_URL: &str = "https://itch.io/user/oauth";
const CLIENT_ID: &str = "85252daf268d27fbefac93e1ac462bfd";
const REDIRECT_URI: &str = "itch://oauth-callback";
const SCOPE: &str = "itch";

pub fn authorization_flow() -> Result<LoginFlow, Error> {
    let state = random_token()?;
    let verifier = random_token()?;
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(&verifier));

    let mut url = Url::parse(AUTHORIZE_URL).expect("authorize URL is valid");
    url.query_pairs_mut().extend_pairs([
        ("response_type", "code"),
        ("client_id", CLIENT_ID),
        ("redirect_uri", REDIRECT_URI),
        ("scope", SCOPE),
        ("state", &state),
        ("code_challenge", &challenge),
        ("code_challenge_method", "S256"),
    ]);

    Cache::set(&verifier_key(&state), &verifier)?;

    Ok(LoginFlow {
        url: url.into(),
        target: REDIRECT_URI.into(),
    })
}

pub async fn complete(redirect: &str) -> Result<String, Error> {
    let redirect =
        Url::parse(redirect).map_err(|e| Error::auth(format!("invalid OAuth redirect: {e}")))?;
    let param = |name: &str| {
        redirect
            .query_pairs()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.into_owned())
    };

    if let Some(error) = param("error") {
        return Err(Error::auth(format!("authorization failed: {error}")));
    }

    let state = param("state").ok_or(Error::auth("OAuth redirect is missing `state`"))?;
    let verifier: String = Cache::get(&verifier_key(&state))
        .ok_or(Error::auth("unknown OAuth state, please try again"))?;
    let code = param("code").ok_or(Error::auth("OAuth redirect is missing `code`"))?;

    exchange_code(&code, &verifier).await
}

async fn exchange_code(code: &str, verifier: &str) -> Result<String, Error> {
    let body = form_urlencoded::Serializer::new(String::new())
        .extend_pairs([
            ("grant_type", "authorization_code"),
            ("code", code),
            ("code_verifier", verifier),
            ("redirect_uri", REDIRECT_URI),
            ("client_id", CLIENT_ID),
        ])
        .finish();

    let request = api::post("/oauth/token")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body);

    let token: Token = api::send(request).await?;
    Ok(token.key.key)
}

fn verifier_key(state: &str) -> String {
    format!("oauth/{state}")
}

fn random_token() -> Result<String, Error> {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes).map_err(|e| Error::other(format!("rng failed: {e}")))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

#[derive(Deserialize)]
struct Token {
    key: ApiKey,
}

#[derive(Deserialize)]
struct ApiKey {
    key: String,
}
