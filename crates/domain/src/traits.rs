use anyhow::Result;
use serde_json::Value;

pub trait Storage {
    fn get_plugin_data(&self, id: String) -> Result<Option<Value>>;
    fn set_plugin_data(&self, id: String, data: Value) -> Result<()>;
}
