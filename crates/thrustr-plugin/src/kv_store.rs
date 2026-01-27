use crate::{
    StorefrontProviderError,
    wit::thrustr::storefront::kv_store::{delete, get, list, set},
};

pub use crate::wit::thrustr::storefront::kv_store::Error as KvStoreError;

pub struct KvStore;

impl KvStore {
    pub fn get(key: &str) -> Result<Option<Vec<u8>>, KvStoreError> {
        Ok(get(key)?)
    }

    pub fn get_string(key: &str) -> Result<Option<String>, KvStoreError> {
        Ok(get(key)?.map(|bytes| String::from_utf8(bytes).unwrap()))
    }

    pub fn set(key: &str, value: &[u8]) -> Result<(), KvStoreError> {
        Ok(set(key, value)?)
    }

    pub fn set_string(key: &str, value: &str) -> Result<(), KvStoreError> {
        Ok(set(key, value.as_bytes())?)
    }

    pub fn delete(key: &str) -> Result<(), KvStoreError> {
        Ok(delete(key)?)
    }

    pub fn list(prefix: Option<&str>) -> Result<Vec<String>, KvStoreError> {
        Ok(list(prefix)?)
    }
}

impl From<KvStoreError> for StorefrontProviderError {
    fn from(err: KvStoreError) -> StorefrontProviderError {
        match err {
            KvStoreError::Internal(msg) => StorefrontProviderError::Other(msg),
        }
    }
}
