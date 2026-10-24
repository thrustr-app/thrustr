use crate::plugin::{PluginRuntime, guest_call};
use crate::wit::exports::thrustr::plugin::config;
use anyhow::{bail, ensure};
use async_trait::async_trait;
use domain::component::{ComboboxOption, Config, ConfigSchema, Error, FieldValue, Form};
use std::{collections::HashMap, sync::Arc};

pub struct PluginConfig {
    runtime: Arc<PluginRuntime>,
    indices: Option<config::GuestIndices>,
    schema: Arc<ConfigSchema>,
}

impl PluginConfig {
    pub fn resolve(
        runtime: &Arc<PluginRuntime>,
        schema: Option<Arc<ConfigSchema>>,
    ) -> anyhow::Result<Option<Self>> {
        let indices = runtime.export("config", config::GuestIndices::new)?;
        let Some(schema) = schema else {
            ensure!(
                indices.is_none(),
                "plugin exports Config but its manifest has no [config]"
            );
            return Ok(None);
        };
        if indices.is_none()
            && let Some(field) = schema.fields().find(|field| field.is_dynamic())
        {
            bail!(
                "field `{}` has dynamic options but the plugin does not export Config",
                field.id()
            );
        }
        Ok(Some(Self {
            runtime: runtime.clone(),
            indices,
            schema,
        }))
    }
}

#[async_trait]
impl Config for PluginConfig {
    fn schema(&self) -> &ConfigSchema {
        &self.schema
    }

    async fn validate(&self, fields: HashMap<String, FieldValue>) -> Result<(), Error> {
        let Some(indices) = &self.indices else {
            return Ok(());
        };
        let fields = fields
            .into_iter()
            .map(|(id, value)| (id, value.into()))
            .collect();
        guest_call!(self.runtime, indices, |config, accessor| {
            config.call_validate(accessor, fields)
        })
    }

    async fn options(&self, field_id: &str) -> Result<Vec<ComboboxOption>, Error> {
        let Some(indices) = &self.indices else {
            return Err(Error::Other("plugin does not export Config".to_owned()));
        };
        let field_id = field_id.to_owned();
        let options = guest_call!(self.runtime, indices, |config, accessor| {
            config.call_options(accessor, field_id)
        })?;
        Ok(options
            .into_iter()
            .map(|option| ComboboxOption {
                value: option.value,
                label: option.label,
            })
            .collect())
    }
}
