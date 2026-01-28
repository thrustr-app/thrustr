use thrustr_plugin::{
    Storefront, StorefrontProviderError, config::Config, kv_store::KvStore, register_storefront,
};

pub struct EpicGames;

impl Storefront for EpicGames {
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
}

register_storefront!(EpicGames);
