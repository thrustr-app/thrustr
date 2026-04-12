use std::marker::PhantomData;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent, bound = "")]
pub struct Id<T> {
    value: i64,
    _marker: PhantomData<T>,
}

impl<T> From<i64> for Id<T> {
    fn from(value: i64) -> Self {
        Self {
            value,
            _marker: PhantomData,
        }
    }
}

impl<T> From<Id<T>> for i64 {
    fn from(id: Id<T>) -> Self {
        id.value
    }
}

impl<T> Clone for Id<T> {
    fn clone(&self) -> Self { *self }
}

impl<T> Copy for Id<T> {}
