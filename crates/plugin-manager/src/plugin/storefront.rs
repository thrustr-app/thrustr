use crate::{plugin::Plugin, thrustr::plugin::types::Game};
use application::{
    component::{Error, storefront::Storefront},
    domain::game::{GameSource, NewGame},
};
use async_trait::async_trait;

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
    fn to_new_game(&self, game: Game) -> NewGame {
        NewGame {
            name: game.name,
            source: GameSource {
                source_id: self.metadata.id.clone(),
                lookup_id: game.lookup_id,
                external_ids: game.external_ids.into_iter().collect(),
            },
        }
    }
}
