use thrustr_plugin::{StorefrontProvider, StorefrontProviderError, register_storefront_provider};

pub struct EpicGames;

impl StorefrontProvider for EpicGames {
    fn init() -> Result<(), StorefrontProviderError> {
        Ok(())
    }
}

register_storefront_provider!(EpicGames);
