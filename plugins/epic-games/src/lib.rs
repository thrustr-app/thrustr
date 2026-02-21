use serde_json::Value;
use thrustr_plugin::{
    StorefrontProvider, StorefrontProviderError, config::Config, kv_store::KvStore,
    register_storefront_provider,
};

use crate::api::models::AuthResponse;

mod api;

pub struct EpicGames;

impl StorefrontProvider for EpicGames {
    fn init() -> Result<(), StorefrontProviderError> {
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

    fn auth(_url: String, body: Vec<u8>) -> Result<(), StorefrontProviderError> {
        let response: AuthResponse = serde_json::from_slice(&body).unwrap();
        let code = response.authorization_code;
        println!("Authorization code: {}", code);
        Ok(())
    }
}

register_storefront_provider!(EpicGames);
