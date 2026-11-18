use crate::{game::GameId, id::Id};
use anyhow::Result;
use std::{fmt, path::PathBuf};
use strum::{AsRefStr, Display, EnumDiscriminants, EnumString};

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

/// Progress within a phase, in bytes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PhaseProgress {
    pub processed: u64,
    pub total: Option<u64>,
}

#[derive(EnumDiscriminants, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[strum_discriminants(name(DownloadPhaseKind), derive(AsRefStr, Display, EnumString))]
#[strum_discriminants(strum(serialize_all = "lowercase"))]
pub enum DownloadPhase {
    #[default]
    Preparing,
    Downloading(PhaseProgress),
    Installing(PhaseProgress),
    Verifying(PhaseProgress),
    Finalizing,
}

impl DownloadPhase {
    pub fn kind(&self) -> DownloadPhaseKind {
        self.into()
    }

    pub fn progress(&self) -> Option<PhaseProgress> {
        match *self {
            Self::Downloading(progress)
            | Self::Installing(progress)
            | Self::Verifying(progress) => Some(progress),
            Self::Preparing | Self::Finalizing => None,
        }
    }
}

impl fmt::Display for DownloadPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.kind())?;
        match self.progress() {
            Some(PhaseProgress {
                processed,
                total: Some(total),
            }) => write!(f, " ({processed} / {total})"),
            Some(PhaseProgress {
                processed,
                total: None,
            }) => write!(f, " ({processed})"),
            None => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadOutcome {
    Completed,
    Cancelled,
}

pub trait DownloadReporter: Send + Sync {
    /// The current phase and its progress.
    fn report(&self, phase: DownloadPhase);
    /// Bytes received from the network.
    fn received(&self, bytes: u64);
    /// Bytes written to disk.
    fn written(&self, bytes: u64);
}

pub trait DownloadRepository: Send + Sync {
    fn enqueue(&self, download: &NewDownload) -> Result<Download>;
    fn list(&self) -> Result<Vec<Download>>;
    fn save(&self, download: &Download) -> Result<()>;
    fn move_to(&self, id: DownloadId, position: u32) -> Result<()>;
    fn remove(&self, id: DownloadId) -> Result<()>;
}
