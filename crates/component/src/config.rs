use crate::{
    ComponentHandle,
    handle::{OperationError, Permit, Result},
};
use domain::component::{
    ComboboxOption, Config, ConfigOperation, ConfigSchema, Error, Field, FieldValue, Form,
};
use std::{collections::HashMap, sync::Arc};
use tracing::{info, warn};

#[derive(Clone)]
pub struct ConfigHandle {
    config: Arc<dyn Config>,
    component: ComponentHandle,
}

impl ConfigHandle {
    pub(crate) fn new(config: Arc<dyn Config>, component: ComponentHandle) -> Self {
        Self { config, component }
    }

    pub fn component(&self) -> &ComponentHandle {
        &self.component
    }

    pub fn schema(&self) -> &ConfigSchema {
        self.config.schema()
    }

    pub fn values(&self) -> Result<HashMap<String, FieldValue>> {
        let stored = self
            .component
            .context()
            .component_storage
            .get_config_values(self.component.id())?;
        Ok(self.schema().resolve(stored))
    }

    pub async fn options(&self, field_id: &str) -> Result<Vec<ComboboxOption>> {
        let field = match self.schema().field(field_id) {
            Some(Field::Combobox(field)) if field.is_dynamic() => field,
            _ => return Err(OperationError::NoDynamicOptions(field_id.to_owned())),
        };
        let permit = Permit::begin(&self.component, ConfigOperation::LoadOptions)?;

        let result = self.config.options(field_id).await.and_then(|options| {
            field
                .check_dynamic_options(&options)
                .map(|()| options)
                .map_err(|e| Error::Other(format!("invalid options: {e}")))
        });

        permit.finish(result.as_ref().map(drop).map_err(Clone::clone));
        result.map_err(Into::into)
    }

    pub async fn save(&self, mut fields: HashMap<String, FieldValue>) -> Result<()> {
        self.schema().retain_valid(&mut fields)?;
        let permit = Permit::begin(&self.component, ConfigOperation::Save)?;

        let result = self.config.validate(fields.clone()).await;

        if result.is_err() {
            permit.finish(result.clone());
            return result.map_err(Into::into);
        }

        self.component
            .context()
            .component_storage
            .set_config_values(self.component.id(), &fields)
            .inspect_err(|e|  warn!(component = self.component.id(), error = %e, "storing configuration failed"))?;

        info!(component = self.component.id(), "configuration saved");
        permit.finish(Ok(()));
        Ok(())
    }
}
