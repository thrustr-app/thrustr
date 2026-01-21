use thrustr_plugin::{Storefront, register_storefront};

pub struct EpicGames;

impl Storefront for EpicGames {
    fn init() -> Result<(), String> {
        Ok(())
    }
}

register_storefront!(EpicGames);
