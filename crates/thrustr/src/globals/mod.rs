use crate::tokio::Tokio;
use artwork::ArtworkService;
use connectivity::ConnectivityManager;
use gpui::{App, block_on};
use sqlite::SqliteStorage;
use std::sync::Arc;

mod component;
mod game;
mod plugin;

pub use component::ComponentRegistryExt;
pub use game::GameServiceExt;
pub use plugin::PluginServiceExt;

pub fn init(cx: &mut App, storage: Arc<SqliteStorage>) {
    let artwork_repo = storage.clone();
    let artwork_service = block_on(Tokio::spawn(cx, async move {
        let connectivity = ConnectivityManager::builder().build_probing().await;
        ArtworkService::new(connectivity, artwork_repo)
    }))
    .expect("Error initializing connectivity manager");

    let registry = component::init(cx, storage.clone(), storage.clone(), artwork_service);
    plugin::init(cx, storage.clone(), registry);
    game::init(cx, storage);
}
