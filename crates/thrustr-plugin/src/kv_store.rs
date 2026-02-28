use crate::wit::thrustr::plugin::kv_store::{Error, delete, get, list, set};

pub struct KvStore;

impl KvStore {
    pub fn get_bytes(key: &str) -> Result<Option<Vec<u8>>, Error> {
        get(key)
    }

    pub fn get_string(key: &str) -> Result<Option<String>, Error> {
        get(key)?
            .map(|bytes| String::from_utf8(bytes))
            .transpose()
            .map_err(|e| Error::Other(e.to_string()))
    }

    pub fn get_bool(key: &str) -> Result<Option<bool>, Error> {
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

    pub fn set_bytes(key: &str, value: &[u8]) -> Result<(), Error> {
        set(key, value)
    }

    pub fn set_string(key: &str, value: &str) -> Result<(), Error> {
        set(key, value.as_bytes())
    }

    pub fn set_bool(key: &str, value: bool) -> Result<(), Error> {
        let byte = if value { 1u8 } else { 0u8 };
        set(key, &[byte])
    }

    pub fn delete(key: &str) -> Result<(), Error> {
        delete(key)
    }

    pub fn list(prefix: Option<&str>) -> Result<Vec<String>, Error> {
        list(prefix)
    }
}
