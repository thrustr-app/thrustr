use domain::{component::ComponentStorage, game::GameRepository};
use std::sync::Arc;

#[derive(Clone)]
pub struct RegistryContext {
    pub component_storage: Arc<dyn ComponentStorage>,
    pub game_storage: Arc<dyn GameRepository>,
}
