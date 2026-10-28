use crate::{game::GameId, id::Id};
use anyhow::Result;
use std::path::PathBuf;
use strum::{AsRefStr, Display, EnumString};

pub type DownloadId = Id<Download>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Download {
    pub id: DownloadId,
    pub game_id: GameId,
    pub version_path: Vec<String>,
    pub target_dir: PathBuf,
    pub install_size: Option<u64>,
    pub download_size: Option<u64>,
    pub phase: DownloadPhase,
    pub processed: u64,
    pub total: Option<u64>,
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

#[derive(AsRefStr, Display, EnumString, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[strum(serialize_all = "lowercase")]
pub enum DownloadPhase {
    #[default]
    Downloading,
    Installing,
}

pub trait DownloadReporter: Send + Sync {
    /// Bytes of the current phase, counted however suits the storefront.
    fn progress(&self, phase: DownloadPhase, processed: u64, total: Option<u64>);
}

pub trait DownloadRepository: Send + Sync {
    fn enqueue(&self, download: &NewDownload) -> Result<Download>;
    fn list(&self) -> Result<Vec<Download>>;
    fn save(&self, download: &Download) -> Result<()>;
    fn move_to(&self, id: DownloadId, position: u32) -> Result<()>;
    fn remove(&self, id: DownloadId) -> Result<()>;
}
