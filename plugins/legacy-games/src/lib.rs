use crate::api::{get_products, giveaway_login, login};
use base64::{Engine, engine::general_purpose::STANDARD};
use pdk::{AuthFlow, Error, Game, Plugin, Storefront, kv_store::KvStore, register_storefront};
use std::collections::HashMap;
use wstd::runtime::block_on;

mod api;
mod error;
mod mapper;

pub struct LegacyGames;

impl Plugin for LegacyGames {
    fn init() -> Result<(), Error> {
        block_on(async move {
            let email: String =
                KvStore::get("email")?.ok_or(Error::Auth("not logged in".into()))?;
            let token = KvStore::get::<String>("token")?;

            match token {
                Some(token) => {
                    login(&token).await?.into_result()?;
                }
                None => giveaway_login(&email).await?.into_result()?,
            }

            Ok(())
        })
    }

    fn login(
        _url: Option<String>,
        _body: Option<String>,
        fields: Option<HashMap<String, String>>,
    ) -> Result<(), Error> {
        block_on(async {
            let fields = fields.ok_or(Error::Auth("fields should not be None".into()))?;
            let email = fields
                .get("email")
                .ok_or(Error::Auth("email is mandatory".into()))?;
            let password = fields.get("password");

            if let Some(password) = password {
                let token = STANDARD.encode(format!("{email}:{password}"));
                let user_id = login(&token).await?.into_result()?.user_id;

                KvStore::set("user_id", &user_id)?;
                KvStore::set("token", &token)?;
            } else {
                giveaway_login(email).await?.into_result()?;
            }

            KvStore::set("email", email)?;

            Ok(())
        })
    }

    fn logout() -> Result<(), Error> {
        KvStore::delete("email")?;
        KvStore::delete("token")?;
        Ok(())
    }

    fn validate_config(_fields: HashMap<String, String>) -> Result<(), Error> {
        Ok(())
    }

    fn get_login_flow() -> Result<Option<AuthFlow>, Error> {
        Ok(None)
    }

    fn get_logout_flow() -> Result<Option<AuthFlow>, Error> {
        Ok(None)
    }
}

impl Storefront for LegacyGames {
    fn list_games() -> Result<Vec<Game>, Error> {
        block_on(async {
            let email: String =
                KvStore::get("email")?.ok_or(Error::Auth("not logged in".into()))?;
            let token = KvStore::get::<String>("token")?;
            let user_id = KvStore::get("user_id")?;

            let games = get_products(&email, token.as_deref(), user_id)
                .await?
                .into_iter()
                .flat_map(Vec::<Game>::from)
                .collect();

            Ok(games)
        })
    }
}

register_storefront!(LegacyGames);
