use crate::{game::GameId, id::Id};
use anyhow::Result;
use std::path::PathBuf;

pub type DownloadId = Id<Download>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Download {
    pub id: DownloadId,
    pub game_id: GameId,
    pub version_path: Vec<String>,
    pub target_dir: PathBuf,
    pub install_size: Option<u64>,
    pub download_size: Option<u64>,
    pub downloaded: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewDownload {
    pub game_id: GameId,
    pub version_path: Vec<String>,
    pub target_dir: PathBuf,
    pub install_size: Option<u64>,
    pub download_size: Option<u64>,
}

pub trait DownloadRepository: Send + Sync {
    fn enqueue(&self, download: &NewDownload) -> Result<Download>;
    fn list(&self) -> Result<Vec<Download>>;
    fn save(&self, download: &Download) -> Result<()>;
    fn move_to(&self, id: DownloadId, position: u32) -> Result<()>;
    fn remove(&self, id: DownloadId) -> Result<()>;
}
