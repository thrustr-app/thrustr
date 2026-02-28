use thrustr_plugin::{Plugin, PluginError, StorefrontProvider, register_storefront_provider};
use wasip2::{clocks::monotonic_clock, io::poll};

pub struct LegacyGames;

impl Plugin for LegacyGames {
    fn init() -> Result<(), PluginError> {
        let pollable = monotonic_clock::subscribe_duration(8_000_000_000);
        poll::poll(&[&pollable]);
        Ok(())
    }
}

impl StorefrontProvider for LegacyGames {
    fn test() -> Result<(), PluginError> {
        Ok(())
    }
}

register_storefront_provider!(LegacyGames);
