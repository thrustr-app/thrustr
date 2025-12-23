use extism::convert::Json;
use models::SetPluginDataInput;
use serde_json::{Map, Value};

pub fn get_plugin_data(key: String) -> Json<Map<String, Value>> {
    let json_test = r#"{ "a": 1, "b": true }"#;
    let map: Map<String, Value> = serde_json::from_str(json_test)
        .expect("Failed to parse database JSON. Is the database corrupted?");

    Json(map)
}

pub fn set_plugin_data(input: Json<SetPluginDataInput>) -> bool {
    println!("set_plugin_data called with input: {:?}", input);
    true
}
