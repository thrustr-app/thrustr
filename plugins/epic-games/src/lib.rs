use serde_json::Value;
use thrustr_plugin::{
    StorefrontProvider, StorefrontProviderError, config::Config, kv_store::KvStore,
    register_storefront_provider,
};
use wasip2::{clocks::monotonic_clock, io::poll};

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

        let pollable = monotonic_clock::subscribe_duration(10_000_000_000);
        poll::poll(&[&pollable]);

        KvStore::set_string("login", "lololol")?;

        if let Some(exists) = KvStore::get_string("login")? {
            println!("Exists: {}", exists);
        }

        Ok(())
    }
}

register_storefront_provider!(EpicGames);
