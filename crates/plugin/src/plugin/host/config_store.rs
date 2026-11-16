use crate::plugin::PluginState;
use crate::wit::thrustr::plugin::{
    config_store::{Error as ConfigStoreError, Host as ConfigStoreHost},
    types::FieldValue,
};
use domain::component::Form;

impl ConfigStoreHost for PluginState {
    fn get(&mut self, field_id: String) -> Result<FieldValue, ConfigStoreError> {
        let runtime = &self.runtime;
        let Some(field) = runtime
            .config_schema
            .as_deref()
            .and_then(|schema| schema.field(&field_id))
        else {
            return Err(ConfigStoreError::Other(format!(
                "`{field_id}` is not a declared config field"
            )));
        };

        let stored = runtime
            .storage
            .get_config_value(runtime.id.as_str(), &field_id)
            .map_err(|e| ConfigStoreError::Other(format!("{e:#}")))?;

        Ok(field
            .resolve(stored)
            .unwrap_or_else(|| field.empty_value())
            .into())
    }
}
