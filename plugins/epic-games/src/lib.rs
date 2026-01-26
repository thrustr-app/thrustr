use thrustr_plugin::{KvStore, Storefront, register_storefront};

pub struct EpicGames;

impl Storefront for EpicGames {
    fn init() -> Result<(), String> {
        KvStore::set_string("login", "lololol").unwrap();
        let result = KvStore::get_string("login");
        println!("{:?}", result);
        Ok(())
    }
}

register_storefront!(EpicGames);
