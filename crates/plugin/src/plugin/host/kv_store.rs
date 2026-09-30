use crate::{
    plugin::PluginState,
    wit::thrustr::plugin::kv_store::{Error as KvStoreError, Host as KvStoreHost},
};
use std::time::{Duration, SystemTime};

impl KvStoreHost for PluginState {
    fn get(&mut self, key: String) -> Result<Option<Vec<u8>>, KvStoreError> {
        self.storage
            .get_data(&self.id, &key)
            .map_err(|e| KvStoreError::Other(e.to_string()))
    }

    fn set(
        &mut self,
        key: String,
        value: Vec<u8>,
        ttl_ms: Option<u64>,
    ) -> Result<(), KvStoreError> {
        let expires_at =
            ttl_ms.and_then(|ttl_ms| SystemTime::now().checked_add(Duration::from_millis(ttl_ms)));
        self.storage
            .set_data(&self.id, &key, &value, expires_at)
            .map_err(|e| KvStoreError::Other(e.to_string()))
    }

    fn delete(&mut self, key: String) -> Result<(), KvStoreError> {
        self.storage
            .delete_data(&self.id, &key)
            .map_err(|e| KvStoreError::Other(e.to_string()))
    }

    fn list(&mut self, prefix: Option<String>) -> Result<Vec<String>, KvStoreError> {
        self.storage
            .list_data(&self.id, prefix.as_deref())
            .map_err(|e| KvStoreError::Other(e.to_string()))
    }
}
