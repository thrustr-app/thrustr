use crate::api::{get_giveaway_products, giveaway_login, login};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::collections::HashMap;
use thrustr_plugin::{
    AuthFlow, Error, Game, Plugin, Storefront, kv_store::KvStore, register_storefront,
};

mod api;
mod error;
mod mapper;

pub struct LegacyGames;

impl Plugin for LegacyGames {
    fn init() -> Result<(), Error> {
        let email: String = KvStore::get("email")?.ok_or(Error::Auth("not logged in".into()))?;
        let token = KvStore::get::<String>("token")?;

        match token {
            Some(token) => {
                login(&token)?.into_result()?;
            }
            None => giveaway_login(&email)?.into_result()?,
        }

        Self::list_games()?;

        Ok(())
    }

    fn login(
        _url: Option<String>,
        _body: Option<String>,
        fields: Option<HashMap<String, String>>,
    ) -> Result<(), Error> {
        let fields = fields.ok_or(Error::Auth("fields should not be None".into()))?;
        let email = fields
            .get("email")
            .ok_or(Error::Auth("email is mandatory".into()))?;
        let password = fields.get("password");

        if let Some(password) = password {
            let token = STANDARD.encode(format!("{email}:{password}"));
            let user_id = login(&token)?.into_result()?.user_id;

            KvStore::set("user_id", &user_id)?;
            KvStore::set("token", &token)?;
        } else {
            giveaway_login(email)?.into_result()?;
        }

        KvStore::set("email", email)?;

        Ok(())
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
        let email: String = KvStore::get("email")?.ok_or(Error::Auth("not logged in".into()))?;

        let products = get_giveaway_products(&email)?.into_result()?;

        Ok(products.into_iter().flat_map(Vec::<Game>::from).collect())
    }
}

register_storefront!(LegacyGames);
