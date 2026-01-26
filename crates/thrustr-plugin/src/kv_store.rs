use crate::wit::thrustr::storefront::kv_store::{Error as KvError, get, set};

pub struct KvStore;

impl KvStore {
    pub fn get(key: &str) -> Result<Vec<u8>, KvError> {
        Ok(get(key)?)
    }

    pub fn get_string(key: &str) -> Result<String, KvError> {
        let bytes = get(key)?;
        let string = String::from_utf8(bytes).expect("Invalid UTF-8.");
        Ok(string)
    }

    pub fn set(key: &str, value: &[u8]) -> Result<(), KvError> {
        Ok(set(key, value)?)
    }

    pub fn set_string(key: &str, value: &str) -> Result<(), KvError> {
        Ok(set(key, value.as_bytes())?)
    }
}
