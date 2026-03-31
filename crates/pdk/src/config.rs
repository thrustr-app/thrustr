use crate::wit::thrustr::plugin::config::Error;
use crate::wit::thrustr::plugin::config::get;

pub struct Config;

impl Config {
    pub fn get(field_id: &str) -> Result<String, Error> {
        get(field_id)
    }
}
