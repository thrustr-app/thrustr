mod pdk;

use extism_pdk::*;
use pdk::*;
use serde_json::{Map, Value};

// Returns the plugin manifest metadata.
// This includes the plugin id, name, authors, and semantic version.
pub(crate) fn manifest() -> Result<types::Manifest, Error> {
    get_plugin_data("epic-games".to_string()).unwrap();

    let test_map: Map<String, Value> = serde_json::from_str(
        r#"{
        "favorite_game": "Fortnite",
        "owned_games": ["Fortnite", "Rocket League", "Gears of War"]
    }"#,
    )
    .unwrap();

    set_plugin_data(types::SetPluginDataInput {
        id: "epic-games".to_string(),
        data: test_map,
    })
    .unwrap();

    Ok(types::Manifest {
        authors: vec!["Jorge Pardo".to_string()],
        id: "epic-games".to_string(),
        name: "Epic Games Storefront Plugin".to_string(),
        version: "1.0.0".to_string(),
        description: "A plugin to interact with the Epic Games Store.".to_string(),
    })
}
