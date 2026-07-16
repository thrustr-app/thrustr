use crate::tokio::Tokio;
use artwork::ArtworkService;
use connectivity::ConnectivityManager;
use gpui::App;
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

    let connectivity = ConnectivityManager::builder(tokio_handle.clone()).build_probing();
    let artwork_service = ArtworkService::new(tokio_handle, connectivity, artwork_repo, game_repo);

    artwork_global::init(cx, artwork_service.clone());
    artwork_service.trigger_backfill();

    let registry = component::init(cx, storage.clone(), storage.clone(), artwork_service);
    plugin::init(cx, storage.clone(), registry);
    game::init(cx, storage);
}
