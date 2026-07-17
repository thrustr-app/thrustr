use crate::api::models::Product;
use pdk::Game;
use std::collections::BTreeMap;

impl From<Product> for Vec<Game> {
    fn from(product: Product) -> Self {
        product
            .games
            .into_iter()
            .map(|g| Game {
                name: g.game_name,
                lookup_id: g.installer_uuid,
                external_ids: BTreeMap::from_iter([
                    ("game_id".into(), g.game_id),
                    ("product_id".into(), product.id.to_string()),
                ]),
                cover_url: Some(g.game_coverart),
                summary: Some(g.game_description),
                description: None,
            })
            .collect()
    }
}
