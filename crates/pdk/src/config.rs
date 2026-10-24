use crate::wit::config::thrustr::plugin::config_store::get;
use crate::{Error, FieldValue};

pub struct Config;

impl Config {
    /// Value of a text field or single selection combobox. Empty when unset.
    pub fn get(field_id: &str) -> Result<String, Error> {
        match get(field_id)? {
            FieldValue::Text(text) => Ok(text),
            FieldValue::List(_) => Err(mismatch(field_id, "a list")),
        }
    }

    /// Values of a multiple selection combobox. Empty when unset.
    pub fn get_list(field_id: &str) -> Result<Vec<String>, Error> {
        match get(field_id)? {
            FieldValue::List(values) => Ok(values),
            FieldValue::Text(_) => Err(mismatch(field_id, "text")),
        }
    }
}

fn mismatch(field_id: &str, actual: &str) -> Error {
    Error::other(format!("`{field_id}` holds {actual}"))
}
