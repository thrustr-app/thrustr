use crate::api::{giveaway_login, wp_login};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::collections::HashMap;
use thrustr_plugin::{AuthFlow, Error, Plugin, Storefront, kv_store::KvStore, register_storefront};

mod api;
mod error;

pub struct LegacyGames;

impl Plugin for LegacyGames {
    fn init() -> Result<(), Error> {
        let email = KvStore::get_string("email")?.ok_or(Error::Auth("not logged in".into()))?;
        let token = KvStore::get_string("token")?;

        match token {
            Some(token) => wp_login(&token)?.into_result()?,
            None => giveaway_login(&email)?.into_result()?,
        }

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
            wp_login(&token)?.into_result()?;

            KvStore::set_string("token", &token)?;
        } else {
            giveaway_login(email)?.into_result()?;
        }

        KvStore::set_string("email", email)?;

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
    fn test() -> Result<(), Error> {
        Ok(())
    }
}

register_storefront!(LegacyGames);
