use crate::wit::thrustr::storefront::kv_store::{Error as KvError, get, set};

pub struct KvStore;

impl KvStore {
    pub fn get(key: &str) -> Result<Option<Vec<u8>>, KvError> {
        Ok(get(key)?)
    }

    pub fn get_string(key: &str) -> Result<Option<String>, KvError> {
        Ok(get(key)?.map(|bytes| String::from_utf8(bytes).unwrap()))
    }

    pub fn set(key: &str, value: &[u8]) -> Result<(), KvError> {
        Ok(set(key, value)?)
    }

    pub fn set_string(key: &str, value: &str) -> Result<(), KvError> {
        Ok(set(key, value.as_bytes())?)
    }
}
