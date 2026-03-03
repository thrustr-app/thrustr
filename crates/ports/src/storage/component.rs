use anyhow::Result;

pub trait ComponentStorage: Send + Sync {
    fn get_data(&self, component_id: &str, key: &str) -> Result<Option<Vec<u8>>>;

    fn set_data(&self, component_id: &str, key: &str, value: Vec<u8>) -> Result<()>;

    fn delete_data(&self, component_id: &str, key: &str) -> Result<()>;

    fn list_data(&self, component_id: &str, prefix: Option<&str>) -> Result<Vec<String>>;

    // None if not found. If empty value -> Empty string ""
    fn get_config(&self, component_id: &str, field_id: &str) -> Result<Option<String>>;
}
