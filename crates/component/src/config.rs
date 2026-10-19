use crate::{
    ComponentHandle,
    handle::{Permit, Result},
};
use domain::component::{Config, ConfigOperation, ConfigSchema, FieldValue, Form};
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

    pub async fn save(&self, mut fields: HashMap<String, FieldValue>) -> Result<()> {
        self.schema().retain_valid(&mut fields)?;
        let permit = Permit::begin(&self.component, ConfigOperation::Save)?;

        let result = self.config.validate(fields.clone()).await;

        if result.is_err() {
            permit.finish(result.clone()).await?;
            return result.map_err(Into::into);
        }

        self.component
            .context()
            .component_storage
            .set_config_values(self.component.id(), &fields)
            .inspect_err(|e|  warn!(component = self.component.id(), error = %e, "storing configuration failed"))?;

        info!(component = self.component.id(), "configuration saved");
        permit.finish(Ok(())).await
    }
}
