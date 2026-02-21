use anyhow::Result;

pub trait ExtensionStorage: Send + Sync {
    fn get_data(&self, extension_id: &str, key: &str) -> Result<Option<Vec<u8>>>;

    fn set_data(&self, extension_id: &str, key: &str, value: Vec<u8>) -> Result<()>;

    fn delete_data(&self, extension_id: &str, key: &str) -> Result<()>;

    fn list_data(&self, extension_id: &str, prefix: Option<&str>) -> Result<Vec<String>>;

    // None if not found. If empty value -> Empty string ""
    fn get_config(&self, extension_id: &str, field_id: &str) -> Result<Option<String>>;
}
