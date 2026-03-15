use crate::api::models::Product;
use thrustr_plugin::Game;

impl From<Product> for Vec<Game> {
    fn from(product: Product) -> Self {
        product
            .games
            .into_iter()
            .map(|g| Game { name: g.game_name })
            .collect()
    }
}
