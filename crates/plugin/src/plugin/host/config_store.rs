use crate::plugin::PluginState;
use crate::wit::thrustr::plugin::{
    config_store::{Error as ConfigStoreError, Host as ConfigStoreHost},
    types::FieldValue,
};
use domain::component::Form;

impl ConfigStoreHost for PluginState {
    fn get(&mut self, field_id: String) -> Result<FieldValue, ConfigStoreError> {
        let Some(field) = self
            .config_schema
            .as_deref()
            .and_then(|schema| schema.field(&field_id))
        else {
            return Err(ConfigStoreError::Other(format!(
                "`{field_id}` is not a declared config field"
            )));
        };

        let stored = self
            .storage
            .get_config_value(&self.id, &field_id)
            .map_err(|e| ConfigStoreError::Other(e.to_string()))?;

        Ok(field
            .resolve(stored)
            .unwrap_or_else(|| field.empty_value())
            .into())
    }
}
