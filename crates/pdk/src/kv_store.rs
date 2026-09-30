use crate::{Error, wit::plugin::thrustr::plugin::kv_store};
use serde::{Serialize, de::DeserializeOwned};
use std::time::Duration;

/// Persistent key-value store for storing arbitrary data.
pub struct KvStore;

impl KvStore {
    pub fn get<T: DeserializeOwned>(key: &str) -> Result<Option<T>, Error> {
        kv_store::get(key)?
            .map(|bytes| {
                postcard::from_bytes(&bytes)
                    .map_err(|e| Error::other(format!("decoding kv entry `{key}` failed: {e}")))
            })
            .transpose()
    }

    pub fn set<T: Serialize + ?Sized>(key: &str, value: &T) -> Result<(), Error> {
        kv_store::set(key, &encode(key, value)?, None)
    }

    pub fn set_with_ttl<T: Serialize + ?Sized>(
        key: &str,
        value: &T,
        ttl: Duration,
    ) -> Result<(), Error> {
        let ttl_ms = u64::try_from(ttl.as_millis()).unwrap_or(u64::MAX);
        kv_store::set(key, &encode(key, value)?, Some(ttl_ms))
    }

    pub fn delete(key: &str) -> Result<(), Error> {
        kv_store::delete(key)
    }

    pub fn list(prefix: Option<&str>) -> Result<Vec<String>, Error> {
        kv_store::list(prefix)
    }
}

fn encode<T: Serialize + ?Sized>(key: &str, value: &T) -> Result<Vec<u8>, Error> {
    postcard::to_allocvec(value)
        .map_err(|e| Error::other(format!("encoding kv entry `{key}` failed: {e}")))
}
