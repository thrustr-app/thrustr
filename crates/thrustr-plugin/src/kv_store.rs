use crate::{
    StorefrontProviderError,
    wit::thrustr::plugin::kv_store::{delete, get, list, set},
};

pub use crate::wit::thrustr::plugin::kv_store::Error as KvStoreError;

pub struct KvStore;

impl KvStore {
    pub fn get_bytes(key: &str) -> Result<Option<Vec<u8>>, KvStoreError> {
        Ok(get(key)?)
    }

    pub fn get_string(key: &str) -> Result<Option<String>, KvStoreError> {
        Ok(get(key)?.map(|bytes| String::from_utf8(bytes).unwrap()))
    }

    pub fn get_bool(key: &str) -> Result<Option<bool>, KvStoreError> {
        Ok(get(key)?.map(|bytes| {
            if bytes.len() != 1 {
                panic!("Expected 1 byte for boolean value");
            }
            match bytes[0] {
                0 => false,
                1 => true,
                _ => panic!("Invalid byte value for boolean: {}", bytes[0]),
            }
        }))
    }

    pub fn set_bytes(key: &str, value: &[u8]) -> Result<(), KvStoreError> {
        Ok(set(key, value)?)
    }

    pub fn set_string(key: &str, value: &str) -> Result<(), KvStoreError> {
        Ok(set(key, value.as_bytes())?)
    }

    pub fn set_bool(key: &str, value: bool) -> Result<(), KvStoreError> {
        let byte = if value { 1u8 } else { 0u8 };
        Ok(set(key, &[byte])?)
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
