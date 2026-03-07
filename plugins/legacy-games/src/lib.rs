use thrustr_plugin::{Plugin, PluginError, Storefront, register_storefront};
use wasip2::{clocks::monotonic_clock, io::poll};

pub struct LegacyGames;

impl Plugin for LegacyGames {
    fn init() -> Result<(), PluginError> {
        let pollable = monotonic_clock::subscribe_duration(8_000_000_000);
        poll::poll(&[&pollable]);
        Ok(())
    }

    fn validate_config(_fields: Vec<(String, String)>) -> Result<(), PluginError> {
        Ok(())
    }
}

impl Storefront for LegacyGames {
    fn test() -> Result<(), PluginError> {
        Ok(())
    }
}

register_storefront!(LegacyGames);
