use thrustr_plugin::{StorefrontProvider, StorefrontProviderError, register_storefront_provider};
use wasip2::{clocks::monotonic_clock, io::poll};

pub struct EpicGames;

impl StorefrontProvider for EpicGames {
    fn init() -> Result<(), StorefrontProviderError> {
        let pollable = monotonic_clock::subscribe_duration(8_000_000_000);
        poll::poll(&[&pollable]);
        Ok(())
    }
}

register_storefront_provider!(EpicGames);
