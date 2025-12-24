use crate::SharedStorage;
use serde_json::{Map, Value};

pub fn get_plugin_data(storage: &SharedStorage, plugin_id: String) -> Option<Map<String, Value>> {
    storage.get_plugin_data(plugin_id).unwrap().and_then(|v| {
        if let Value::Object(map) = v {
            Some(map)
        } else {
            None
        }
    })
}

pub fn set_plugin_data(
    storage: &SharedStorage,
    plugin_id: String,
    data: Map<String, Value>,
) -> bool {
    storage
        .set_plugin_data(plugin_id, Value::Object(data))
        .unwrap();
    true
}
