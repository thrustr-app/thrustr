use crate::api::{endpoints::auth_url, models::AuthResponse};
use thrustr_plugin::{
    AuthFlow, Plugin, PluginError, Storefront, config::Config, kv_store::KvStore,
    register_storefront,
};

mod api;

pub struct EpicGames;

impl Plugin for EpicGames {
    fn init() -> Result<(), PluginError> {
        let username = Config::get("username")?;
        if username.is_empty() {
            return Err(PluginError::Configuration(
                "Username cannot be empty".into(),
            ));
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

    fn get_login_flow() -> Result<Option<AuthFlow>, PluginError> {
        Ok(Some(AuthFlow {
            url: auth_url(),
            target: "https://www.epicgames.com/id/api/redirect?".into(),
        }))
    }

    fn get_logout_flow() -> Result<Option<AuthFlow>, PluginError> {
        Ok(Some(AuthFlow {
            url: "https://www.epicgames.com/id/logout?productName=epic-games&redirectUrl=https://www.epicgames.com/id/login".into(),
            target: "https://www.epicgames.com/id/login".into(),
        }))
    }

    fn login(url: String, body: String) -> Result<(), PluginError> {
        println!("got url: {:?}", url);
        println!("got body: {:?}", body);
        Ok(())
    }

    fn logout(url: String, body: String) -> Result<(), PluginError> {
        // delete tokens and such.
        Ok(())
    }

    fn validate_config(fields: Vec<(String, String)>) -> Result<(), PluginError> {
        let username = fields
            .iter()
            .find(|(id, _)| id == "username")
            .map(|(_, v)| v.as_str());
        if username.is_none() || username.unwrap().is_empty() {
            return Err(PluginError::Configuration(
                "Username cannot be empty".into(),
            ));
        }

        Ok(())
    }
}

impl Storefront for EpicGames {
    fn test() -> Result<(), PluginError> {
        Ok(())
    }
}

register_storefront!(EpicGames);
