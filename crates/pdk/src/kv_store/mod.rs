use crate::{
    kv_store::value::KvValue,
    wit::thrustr::plugin::kv_store::{Error, delete, get, list, set},
};

#[macro_use]
mod value;

pub struct KvStore;

impl KvStore {
    pub fn get<T: KvValue>(key: &str) -> Result<Option<T>, Error> {
        get(key)?.map(T::from_bytes).transpose()
    }

    pub fn set<T: KvValue>(key: &str, value: &T) -> Result<(), Error> {
        set(key, &value.as_bytes())
    }

    pub fn list(prefix: Option<&str>) -> Result<Vec<String>, Error> {
        list(prefix)
    }

    pub fn delete(key: &str) -> Result<(), Error> {
        delete(key)
    }
}
