use crate::plugin::PluginState;
use crate::wit::thrustr::plugin::config_store::{
    Error as ConfigStoreError, Host as ConfigStoreHost,
};

impl ConfigStoreHost for PluginState {
    fn get(&mut self, field_id: String) -> Result<String, ConfigStoreError> {
        self.storage
            .get_config_value(&self.id, &field_id)
            .map(Option::unwrap_or_default)
            .map_err(|e| ConfigStoreError::Other(e.to_string()))
    }
}
