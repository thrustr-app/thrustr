use crate::plugin::PluginState;
use crate::wit::thrustr::plugin::config::{Error as ConfigError, Host as ConfigHost};

impl ConfigHost for PluginState {
    async fn get(&mut self, field_id: String) -> Result<String, ConfigError> {
        self.storage
            .get_config_value(&self.id, &field_id)
            .map(|v| v.unwrap_or_default())
            .map_err(|e| ConfigError::Other(e.to_string()))
    }
}
