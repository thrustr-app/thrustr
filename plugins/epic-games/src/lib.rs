mod pdk;

use extism_pdk::*;
use pdk::*;
use serde_json::{Map, Value};

pub(crate) fn initialize() -> Result<(), Error> {
    get_plugin_data().unwrap();

    let test_map: Map<String, Value> = serde_json::from_str(
        r#"{
        "favorite_game": "Fortnite",
        "owned_games": ["Fortnite", "Rocket League", "Gears of War"]
    }"#,
    )
    .unwrap();

    set_plugin_data(test_map).unwrap();
    Ok(())
}
