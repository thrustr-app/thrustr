use crate::{
    plugin::PluginState,
    wit::thrustr::plugin::kv_store::{Error as KvStoreError, Host as KvStoreHost},
};
use std::time::{Duration, SystemTime};

impl KvStoreHost for PluginState {
    fn get(&mut self, key: String) -> Result<Option<Vec<u8>>, KvStoreError> {
        let runtime = &self.runtime;
        Ok(runtime.storage.get_data(runtime.id.as_str(), &key)?)
    }

    fn set(
        &mut self,
        key: String,
        value: Vec<u8>,
        ttl_ms: Option<u64>,
    ) -> Result<(), KvStoreError> {
        let expires_at =
            ttl_ms.and_then(|ttl_ms| SystemTime::now().checked_add(Duration::from_millis(ttl_ms)));
        let runtime = &self.runtime;
        Ok(runtime
            .storage
            .set_data(runtime.id.as_str(), &key, &value, expires_at)?)
    }

    fn delete(&mut self, key: String) -> Result<(), KvStoreError> {
        let runtime = &self.runtime;
        Ok(runtime.storage.delete_data(runtime.id.as_str(), &key)?)
    }

    fn list(&mut self, prefix: String) -> Result<Vec<String>, KvStoreError> {
        let runtime = &self.runtime;
        Ok(runtime.storage.list_data(runtime.id.as_str(), &prefix)?)
    }
}

impl From<anyhow::Error> for KvStoreError {
    fn from(error: anyhow::Error) -> Self {
        Self::Other(format!("{error:#}"))
    }
}
