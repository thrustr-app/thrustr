mod pdk;

use extism_pdk::*;
use pdk::*;

// Returns the plugin manifest metadata.
// This includes the plugin id, name, authors, and semantic version.
pub(crate) fn manifest() -> Result<types::Manifest, Error> {
    Ok(types::Manifest {
        authors: vec!["Jorge Pardo".to_string()],
        id: "epic-games".to_string(),
        name: "Epic Games Storefront Plugin".to_string(),
        version: "1.0.0".to_string(),
        description: "A plugin to interact with the Epic Games Store.".to_string(),
    })
}
