use crate::api::models::Product;
use thrustr_plugin::Game;

// For games with missing names.
// Format: ((product_id, game_id), name)
const NAME_MAPPINGS: [((u64, &str), &str); 1] = [(
    (1838990, "6cb8117d-19da-4247-a7f7-4358c1e4a096"),
    "Lila’s Sky Ark",
)];

impl From<Product> for Vec<Game> {
    fn from(product: Product) -> Self {
        product
            .games
            .into_iter()
            .map(|g| {
                let name = if g.game_name.is_empty() {
                    NAME_MAPPINGS
                        .iter()
                        .find(|((pid, gid), _)| *pid == product.id && *gid == g.game_id)
                        .map(|(_, name)| name.to_string())
                        .unwrap_or_default()
                } else {
                    g.game_name
                };

                Game {
                    name,
                    lookup_id: format!("{}-{}", product.id, g.game_id),
                    external_ids: vec![
                        ("game_id".into(), g.game_id),
                        ("product_id".into(), product.id.to_string()),
                        ("installer_uuid".into(), g.installer_uuid),
                    ],
                }
            })
            .collect()
    }
}
