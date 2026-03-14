use std::collections::HashMap;

use crate::api::{endpoints::auth_url, models::AuthResponse};
use thrustr_plugin::{
    AuthFlow, Error, Plugin, Storefront, config::Config, kv_store::KvStore, register_storefront,
};

mod api;

pub struct EpicGames;

impl Plugin for EpicGames {
    fn init() -> Result<(), Error> {
        let username = Config::get("username")?;
        if username.is_empty() {
            return Err(Error::Config("Username cannot be empty".into()));
        }

        let some_config = Config::get("username")?;
        println!("Username: {some_config}");

        let list = KvStore::list(None)?;
        println!("{:?}", list);

        KvStore::delete("login")?;

        let list = KvStore::list(None)?;
        println!("{:?}", list);

        KvStore::set_string("login", "lololol")?;

        if let Some(exists) = KvStore::get_string("login")? {
            println!("Exists: {}", exists);
        }

        Ok(())
    }

    fn get_login_flow() -> Result<Option<AuthFlow>, Error> {
        Ok(Some(AuthFlow {
            url: auth_url(),
            target: "https://www.epicgames.com/id/api/redirect?".into(),
        }))
    }

    fn get_logout_flow() -> Result<Option<AuthFlow>, Error> {
        Ok(Some(AuthFlow {
            url: "https://www.epicgames.com/id/logout?productName=epic-games&redirectUrl=https://www.epicgames.com/id/login".into(),
            target: "https://www.epicgames.com/id/login".into(),
        }))
    }

    fn login(
        url: Option<String>,
        body: Option<String>,
        fields: Option<HashMap<String, String>>,
    ) -> Result<(), Error> {
        println!("got url: {:?}", url);
        println!("got body: {:?}", body);
        Ok(())
    }

    fn logout() -> Result<(), Error> {
        // delete tokens and such.
        Ok(())
    }

    fn validate_config(fields: HashMap<String, String>) -> Result<(), Error> {
        Ok(())
    }
}

impl Storefront for EpicGames {
    fn test() -> Result<(), Error> {
        Ok(())
    }
}

register_storefront!(EpicGames);
