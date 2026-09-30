use crate::Error;
use crate::wit::config::thrustr::plugin::config_store::get;

pub struct Config;

impl Config {
    pub fn get(field_id: &str) -> Result<String, Error> {
        get(field_id)
    }
}
