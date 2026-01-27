use anyhow::Result;

pub trait Storage: Send + Sync {
    fn get_plugin_data(&self, plugin_id: &str, key: &str) -> Result<Option<Vec<u8>>>;

    fn set_plugin_data(&self, plugin_id: &str, key: &str, value: Vec<u8>) -> Result<()>;

    fn delete_plugin_data(&self, plugin_id: &str, key: &str) -> Result<()>;

    fn list_plugin_data(&self, plugin_id: &str, prefix: Option<&str>) -> Result<Vec<String>>;
}
