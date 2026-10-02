use std::{
    fs, io,
    path::{Path, PathBuf},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum InstallTargetError {
    #[error("choose an install location")]
    MissingLocation,
    #[error("install location must be an absolute path")]
    RelativeLocation,
    #[error("game name has no characters usable in a folder name")]
    InvalidName,
    #[error("'{}' is not available", .path.display())]
    UnavailableRoot {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("'{}' is not a folder", .0.display())]
    NotADirectory(PathBuf),
    #[error("'{}' already exists and is not empty", .0.display())]
    NotEmpty(PathBuf),
    #[error(transparent)]
    Io(#[from] io::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallTarget {
    pub dir: PathBuf,
    pub available_space: u64,
    pub total_space: u64,
}

impl InstallTarget {
    pub fn resolve(location: &Path, game_name: &str) -> Result<Self, InstallTargetError> {
        if location.as_os_str().is_empty() {
            return Err(InstallTargetError::MissingLocation);
        }
        if !location.is_absolute() {
            return Err(InstallTargetError::RelativeLocation);
        }
        let folder = folder_name(game_name);
        if folder.is_empty() {
            return Err(InstallTargetError::InvalidName);
        }

        let dir = location.join(folder);
        let existing = nearest_existing_dir(&dir)?;
        if existing == dir.as_path() && fs::read_dir(&dir)?.next().is_some() {
            return Err(InstallTargetError::NotEmpty(dir));
        }

        let stats = fs4::statvfs(existing)?;
        Ok(Self {
            available_space: stats.available_space(),
            total_space: stats.total_space(),
            dir,
        })
    }
}

fn nearest_existing_dir(path: &Path) -> Result<&Path, InstallTargetError> {
    for ancestor in path.ancestors() {
        match fs::metadata(ancestor) {
            Ok(metadata) if metadata.is_dir() => return Ok(ancestor),
            Ok(_) => return Err(InstallTargetError::NotADirectory(ancestor.to_path_buf())),
            Err(source) if ancestor.parent().is_none() => {
                return Err(InstallTargetError::UnavailableRoot {
                    path: ancestor.to_path_buf(),
                    source,
                });
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    unreachable!("the last ancestor is always a root")
}

fn folder_name(game_name: &str) -> String {
    game_name.chars().filter(|c| c.is_alphanumeric()).collect()
}
