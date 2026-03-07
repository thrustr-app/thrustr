use anyhow::Result;

pub trait ComponentStorage: Send + Sync {
    fn get_data(&self, component_id: &str, key: &str) -> Result<Option<Vec<u8>>>;

    fn set_data(&self, component_id: &str, key: &str, value: &[u8]) -> Result<()>;

    fn delete_data(&self, component_id: &str, key: &str) -> Result<()>;

    fn list_data(&self, component_id: &str, prefix: Option<&str>) -> Result<Vec<String>>;

    // None if not found. If empty value -> Empty string ""
    fn get_config_value(&self, component_id: &str, field_id: &str) -> Result<Option<String>>;

    fn set_config_value(&self, component_id: &str, field_id: &str, value: &str) -> Result<()>;

    fn set_config_values(&self, component_id: &str, fields: &[(String, String)]) -> Result<()>;
}
