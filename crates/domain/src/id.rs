use serde::{Deserialize, Serialize};
use std::{fmt, marker::PhantomData};

#[derive(Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent, bound = "")]
pub struct Id<T> {
    value: u64,
    _marker: PhantomData<T>,
}

impl<T> From<u64> for Id<T> {
    fn from(value: u64) -> Self {
        Self {
            value,
            _marker: PhantomData,
        }
    }
}

impl<T> From<Id<T>> for u64 {
    fn from(id: Id<T>) -> Self {
        id.value
    }
}

impl<T> Clone for Id<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Id<T> {}

impl<T> fmt::Display for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.value)
    }
}
