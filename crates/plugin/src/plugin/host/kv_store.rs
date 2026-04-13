use crate::{
    plugin::PluginState,
    wit::thrustr::plugin::kv_store::{Error as KvStoreError, Host as KvStoreHost},
};

impl KvStoreHost for PluginState {
    async fn get(&mut self, key: String) -> Result<Option<Vec<u8>>, KvStoreError> {
        self.storage
            .get_data(&self.id, &key)
            .map_err(|e| KvStoreError::Other(e.to_string()))
    }

    async fn set(&mut self, key: String, value: Vec<u8>) -> Result<(), KvStoreError> {
        self.storage
            .set_data(&self.id, &key, &value)
            .map_err(|e| KvStoreError::Other(e.to_string()))
    }

    async fn delete(&mut self, key: String) -> Result<(), KvStoreError> {
        self.storage
            .delete_data(&self.id, &key)
            .map_err(|e| KvStoreError::Other(e.to_string()))
    }

    async fn list(&mut self, prefix: Option<String>) -> Result<Vec<String>, KvStoreError> {
        self.storage
            .list_data(&self.id, prefix.as_deref())
            .map_err(|e| KvStoreError::Other(e.to_string()))
    }
}
