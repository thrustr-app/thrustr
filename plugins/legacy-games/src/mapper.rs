use crate::api::models::Product;
use pdk::Game;

impl From<Product> for Vec<Game> {
    fn from(product: Product) -> Self {
        product
            .games
            .into_iter()
            .map(|g| Game {
                name: g.game_name,
                lookup_id: g.installer_uuid,
                external_ids: vec![
                    ("game_id".into(), g.game_id),
                    ("product_id".into(), product.id.to_string()),
                ],
            })
            .collect()
    }
}
