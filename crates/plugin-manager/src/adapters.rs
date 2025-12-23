use crate::SharedStorage;
use domain::SetPluginDataInput;
use serde_json::{Map, Value};

pub fn get_plugin_data(storage: &SharedStorage, _key: String) -> Map<String, Value> {
    // TODO: Implement actual storage retrieval
    // storage.get_plugin_data(key)
    let json_test = r#"{ "a": 1, "b": true }"#;
    let map: Map<String, Value> = serde_json::from_str(json_test)
        .expect("Failed to parse database JSON. Is the database corrupted?");

    map
}

pub fn set_plugin_data(storage: &SharedStorage, input: SetPluginDataInput) -> bool {
    storage
        .set_plugin_data(input.id, Value::Object(input.data))
        .unwrap();
    true
}
