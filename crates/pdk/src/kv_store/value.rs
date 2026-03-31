use crate::wit::thrustr::plugin::kv_store::Error;
use std::borrow::Cow;

pub trait KvValue: Sized {
    fn as_bytes(&self) -> Cow<'_, [u8]>;
    fn from_bytes(bytes: Vec<u8>) -> Result<Self, Error>;
}

impl KvValue for Vec<u8> {
    fn as_bytes(&self) -> Cow<'_, [u8]> {
        Cow::Borrowed(self)
    }
    fn from_bytes(bytes: Vec<u8>) -> Result<Self, Error> {
        Ok(bytes)
    }
}

impl KvValue for String {
    fn as_bytes(&self) -> Cow<'_, [u8]> {
        Cow::Borrowed(self.as_bytes())
    }

    fn from_bytes(bytes: Vec<u8>) -> Result<Self, Error> {
        String::from_utf8(bytes).map_err(|e| Error::Other(e.to_string()))
    }
}

impl KvValue for bool {
    fn as_bytes(&self) -> Cow<'_, [u8]> {
        Cow::Owned(vec![*self as u8])
    }
    fn from_bytes(bytes: Vec<u8>) -> Result<Self, Error> {
        match bytes.as_slice() {
            [0] => Ok(false),
            [1] => Ok(true),
            _ => Err(Error::Other("invalid bool bytes".into())),
        }
    }
}

macro_rules! impl_kv_number {
    ($($t:ty),*) => {
        $(
            impl KvValue for $t {
                fn as_bytes(&self) -> Cow<'_, [u8]> { Cow::Owned(self.to_le_bytes().to_vec()) }
                fn from_bytes(bytes: Vec<u8>) -> Result<Self, Error> {
                    bytes.try_into()
                        .map(<$t>::from_le_bytes)
                        .map_err(|_| Error::Other(concat!("invalid ", stringify!($t), " bytes").into()))
                }
            }
        )*
    };
}

impl_kv_number!(i8, i16, i32, i64, i128, u8, u16, u32, u64, u128, f32, f64);
