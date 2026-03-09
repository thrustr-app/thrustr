use thrustr_plugin::{AuthFlow, Plugin, PluginError, Storefront, register_storefront};

pub struct LegacyGames;

impl Plugin for LegacyGames {
    fn init() -> Result<(), PluginError> {
        Ok(())
    }

    fn get_login_flow() -> Result<Option<AuthFlow>, PluginError> {
        Ok(None)
    }

    fn get_logout_flow() -> Result<Option<AuthFlow>, PluginError> {
        Ok(None)
    }

    fn login(_url: String, _body: String) -> Result<(), PluginError> {
        Ok(())
    }

    fn logout(_url: String, _body: String) -> Result<(), PluginError> {
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
