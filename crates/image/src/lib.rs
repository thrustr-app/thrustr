use std::path::PathBuf;

mod manager;
mod processing;

pub use manager::ImageManager;

#[derive(Debug, Clone)]
pub struct ImageTask {
    pub url: String,
    pub path: PathBuf,
    pub quality: f32,
}
