use anyhow::Result;
use serde_json::Value;

/// Abstraction over persistent storage.
///
/// Provides basic operations for storing, retrieving, and deleting
/// structured data associated with an identifier. Implementations may
/// use a database, file-based storage, or any other durable backend.
pub trait Storage {
    /// Returns the data associated with the given plugin identifier.
    ///
    /// - `Ok(Some(value))` if data exists for `plugin_id`
    /// - `Ok(None)` if no data is stored for `plugin_id`
    /// - `Err(_)` if the storage operation fails
    fn get_plugin_data(&self, plugin_id: &str) -> Result<Option<Value>>;

    /// Inserts or updates the data associated with the given plugin identifier.
    ///
    /// Returns an error if the storage operation fails.
    fn set_plugin_data(&self, plugin_id: &str, data: Value) -> Result<()>;

    /// Deletes the data associated with the given plugin identifier.
    ///
    /// - `Ok(true)` if data existed and was deleted
    /// - `Ok(false)` if no data existed for `plugin_id`
    /// - `Err(_)` if the storage operation fails
    fn delete_plugin_data(&self, plugin_id: &str) -> Result<bool>;
}
