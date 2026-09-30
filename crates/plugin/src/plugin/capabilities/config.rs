use crate::plugin::{PluginRuntime, guest_call};
use crate::wit::exports::thrustr::plugin::config;
use anyhow::bail;
use async_trait::async_trait;
use domain::component::{Config, ConfigSchema, Error};
use std::{collections::HashMap, sync::Arc};

pub struct PluginConfig {
    runtime: Arc<PluginRuntime>,
    indices: config::GuestIndices,
    schema: ConfigSchema,
}

impl PluginConfig {
    // TODO: maybe it is a good idea to enable the capability if the
    // schema alone is present, since `validate` already defaults to Ok(()).
    pub fn resolve(
        runtime: &Arc<PluginRuntime>,
        schema: Option<ConfigSchema>,
    ) -> anyhow::Result<Option<Self>> {
        let indices = runtime.export("config", config::GuestIndices::new)?;
        Ok(pair(indices, schema)?.map(|(indices, schema)| Self {
            runtime: runtime.clone(),
            indices,
            schema,
        }))
    }
}

fn pair<I, S>(indices: Option<I>, schema: Option<S>) -> anyhow::Result<Option<(I, S)>> {
    match (indices, schema) {
        (Some(indices), Some(schema)) => Ok(Some((indices, schema))),
        (None, None) => Ok(None),
        (Some(_), None) => bail!("plugin exports Config but its manifest has no [config]"),
        (None, Some(_)) => {
            bail!("plugin manifest has [config] but the plugin does not export Config")
        }
    }
}

#[async_trait]
impl Config for PluginConfig {
    fn schema(&self) -> &ConfigSchema {
        &self.schema
    }

    async fn validate(&self, fields: HashMap<String, String>) -> Result<(), Error> {
        guest_call!(self.runtime, self.indices, |config, accessor| {
            config.call_validate(accessor, fields)
        })
    }
}
