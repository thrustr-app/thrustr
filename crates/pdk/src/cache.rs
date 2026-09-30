use crate::{Error, wit::plugin::thrustr::plugin::cache};
use serde::{Serialize, de::DeserializeOwned};
use std::time::Duration;

/// In-memory key-value store for storing arbitrary data.
///
/// This is meant to be used for small and frequently accessed data.
/// If you need to store large amounts of data, such as long API responses
/// or manifests, or persist it between restarts, use [`KvStore`] instead.
///
/// [`KvStore`]: crate::kv_store::KvStore
pub struct Cache;

impl Cache {
    pub fn get<T: DeserializeOwned>(key: &str) -> Option<T> {
        cache::get(key).and_then(|bytes| postcard::from_bytes(&bytes).ok())
    }

    pub fn set<T: Serialize + ?Sized>(key: &str, value: &T) -> Result<(), Error> {
        cache::set(key, &encode(key, value)?, None);
        Ok(())
    }

    pub fn set_with_ttl<T: Serialize + ?Sized>(
        key: &str,
        value: &T,
        ttl: Duration,
    ) -> Result<(), Error> {
        let ttl_ms = u64::try_from(ttl.as_millis()).unwrap_or(u64::MAX);
        cache::set(key, &encode(key, value)?, Some(ttl_ms));
        Ok(())
    }

    pub fn delete(key: &str) {
        cache::delete(key)
    }

    pub fn list(prefix: Option<&str>) -> Vec<String> {
        cache::list(prefix)
    }
}

fn encode<T: Serialize + ?Sized>(key: &str, value: &T) -> Result<Vec<u8>, Error> {
    postcard::to_allocvec(value)
        .map_err(|e| Error::other(format!("encoding cache entry `{key}` failed: {e}")))
}
