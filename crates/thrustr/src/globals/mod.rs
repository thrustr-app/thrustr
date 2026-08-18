use crate::tokio::Tokio;
use artwork::ArtworkService;
use gpui::App;
use net::{ConnectivityConfig, ConnectivityManager};
use sqlite::SqliteStorage;
use std::sync::Arc;

mod artwork_global;
mod component;
mod game;
mod plugin;

pub use artwork_global::ArtworkServiceExt;
pub use component::ComponentRegistryExt;
pub use game::GameServiceExt;
pub use plugin::PluginServiceExt;

pub fn init(cx: &mut App, storage: Arc<SqliteStorage>) {
    let tokio_handle = Tokio::handle(cx);
    let artwork_repo = storage.clone();
    let game_repo = storage.clone();

    let connectivity =
        ConnectivityManager::spawn_probing(tokio_handle.clone(), ConnectivityConfig::default());
    let artwork_service =
        ArtworkService::new(tokio_handle.clone(), connectivity, artwork_repo, game_repo);

    artwork_global::init(cx, artwork_service.clone());
    artwork_service.trigger_backfill();

    let registry = component::init(
        cx,
        tokio_handle,
        storage.clone(),
        storage.clone(),
        artwork_service,
    );
    plugin::init(cx, storage.clone(), registry);
    game::init(cx, storage);
}
