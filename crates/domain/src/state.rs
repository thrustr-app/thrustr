use anyhow::Result;
use serde_json::Value;

pub trait AppStateRepository: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<Value>>;
    fn set(&self, key: &str, value: &Value) -> Result<()>;
    fn delete(&self, key: &str) -> Result<()>;
}
