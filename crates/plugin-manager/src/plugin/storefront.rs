use crate::{plugin::Plugin, thrustr::plugin::types::Game};
use async_trait::async_trait;
use domain::{NewGame, capabilities::storefront::Storefront, component::Error};

#[async_trait]
impl Storefront for Plugin {
    async fn list_games(&self) -> Result<Vec<NewGame>, Error> {
        let (instance, mut store) = self
            .instantiate_storefront()
            .await
            .map_err(|e| Error::Other(format!("{e:?}")))?;

        instance
            .thrustr_plugin_storefront()
            .call_list_games(&mut store)
            .await
            .map_err(|e| Error::Other(format!("Wasm call failed: {e}")))?
            .map_err(Error::from)
            .map(|games| games.into_iter().map(|g| self.to_new_game(g)).collect())
    }
}

impl Plugin {
    fn to_new_game(&self, game: Game) -> domain::NewGame {
        domain::NewGame {
            name: game.name,
            source: domain::GameSource {
                source_id: self.metadata.id.clone(),
                lookup_id: game.lookup_id,
                external_ids: game.external_ids.into_iter().collect(),
            },
        }
    }
}
