use crate::gpui_tokio::Tokio;
use connectivity::ConnectivityManager;
use gpui::{App, block_on};
use image::ImageService;
use sqlite::SqliteStorage;
use std::sync::Arc;

mod component;
mod game;
mod plugin;

pub use component::ComponentRegistryExt;
pub use game::GameServiceExt;
pub use plugin::PluginServiceExt;

pub fn init(cx: &mut App, storage: Arc<SqliteStorage>) {
    let image_service = block_on(Tokio::spawn(cx, async move {
        let connectivity = ConnectivityManager::builder().build_probing().await;
        ImageService::new(connectivity)
    }))
    .expect("Error initializing connectivity manager");

    let registry = component::init(cx, storage.clone(), storage.clone(), image_service);
    plugin::init(cx, storage.clone(), registry);
    game::init(cx, storage);
}
