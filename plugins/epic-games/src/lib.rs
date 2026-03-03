use crate::api::models::AuthResponse;
use serde_json::Value;
use thrustr_plugin::{
    Plugin, PluginError, Storefront, config::Config, kv_store::KvStore, register_storefront,
};
use wasip2::{clocks::monotonic_clock, io::poll};

mod api;

pub struct EpicGames;

impl Plugin for EpicGames {
    fn init() -> Result<(), PluginError> {
        let pollable = monotonic_clock::subscribe_duration(10_000_000_000);
        poll::poll(&[&pollable]);

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
}

impl Storefront for EpicGames {
    fn test() -> Result<(), PluginError> {
        Ok(())
    }
}

register_storefront!(EpicGames);
