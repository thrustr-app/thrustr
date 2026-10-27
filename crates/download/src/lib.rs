mod queue;

pub use queue::Queue;

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
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
                ) => {}
            Err(e) => return Err(e.into()),
        }
    }
    unreachable!("the last ancestor is always a root")
}

fn folder_name(game_name: &str) -> String {
    let kept: String = game_name
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect();
    kept.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    struct Disk(TempDir);

    impl Disk {
        fn new() -> Self {
            Self(TempDir::new().expect("temp dir should be created"))
        }

        fn path(&self, relative: &str) -> PathBuf {
            self.0.path().join(relative)
        }

        fn folder(self, relative: &str) -> Self {
            fs::create_dir_all(self.path(relative)).expect("folder should be created");
            self
        }

        fn file(self, relative: &str) -> Self {
            let path = self.path(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("parent folder should be created");
            }
            fs::write(path, b"").expect("file should be created");
            self
        }
    }

    #[derive(Debug, PartialEq)]
    enum Outcome {
        Installs(PathBuf),
        MissingLocation,
        RelativeLocation,
        InvalidName,
        NotADirectory(PathBuf),
        NotEmpty(PathBuf),
    }

    #[track_caller]
    fn check(location: impl AsRef<Path>, game_name: &str, expected: Outcome) {
        let outcome = match InstallTarget::resolve(location.as_ref(), game_name) {
            Ok(target) => {
                assert!(
                    target.total_space > 0 && target.available_space <= target.total_space,
                    "implausible space for {target:?}"
                );
                Outcome::Installs(target.dir)
            }
            Err(InstallTargetError::MissingLocation) => Outcome::MissingLocation,
            Err(InstallTargetError::RelativeLocation) => Outcome::RelativeLocation,
            Err(InstallTargetError::InvalidName) => Outcome::InvalidName,
            Err(InstallTargetError::NotADirectory(path)) => Outcome::NotADirectory(path),
            Err(InstallTargetError::NotEmpty(path)) => Outcome::NotEmpty(path),
            Err(e) => panic!("unexpected error: {e:?}"),
        };
        assert_eq!(outcome, expected);
    }

    #[track_caller]
    fn check_folder_name(game_name: &str, expected: &str) {
        let disk = Disk::new();
        let location = disk.path("games");
        check(
            &location,
            game_name,
            Outcome::Installs(location.join(expected)),
        );
    }

    #[test]
    fn installs_into_a_folder_named_after_the_game() {
        let disk = Disk::new().folder("games");
        check(
            disk.path("games"),
            "Hades",
            Outcome::Installs(disk.path("games/Hades")),
        );
    }

    #[test]
    fn folder_names_keep_letters_digits_and_single_spaces() {
        check_folder_name("Hollow Knight", "Hollow Knight");
        check_folder_name("Half-Life 2", "HalfLife 2");
        check_folder_name(
            "Baldur's Gate: Enhanced Edition",
            "Baldurs Gate Enhanced Edition",
        );
        check_folder_name("S.T.A.L.K.E.R.", "STALKER");
        check_folder_name("Ōkami HD", "Ōkami HD");
        check_folder_name("  Celeste  ", "Celeste");
        check_folder_name("The   Witcher\t3", "The Witcher 3");
        check_folder_name("Ori - Blind Forest", "Ori Blind Forest");
        check_folder_name("<Inscryption>", "Inscryption");
    }

    #[test]
    fn rejects_names_without_usable_characters() {
        let disk = Disk::new();
        for name in ["", "   ", "?!: -", "..."] {
            check(disk.path("games"), name, Outcome::InvalidName);
        }
    }

    #[test]
    fn requires_a_location() {
        check("", "Hades", Outcome::MissingLocation);
    }

    #[test]
    fn rejects_a_relative_location() {
        check("games", "Hades", Outcome::RelativeLocation);
    }

    #[test]
    fn accepts_a_location_that_does_not_exist_yet() {
        let disk = Disk::new();
        check(
            disk.path("library/games"),
            "Hades",
            Outcome::Installs(disk.path("library/games/Hades")),
        );
        assert!(!disk.path("library").exists(), "resolving created folders");
    }

    #[test]
    fn accepts_an_empty_game_folder() {
        let disk = Disk::new().folder("games/Hades");
        check(
            disk.path("games"),
            "Hades",
            Outcome::Installs(disk.path("games/Hades")),
        );
    }

    #[test]
    fn refuses_a_game_folder_that_is_not_empty() {
        let disk = Disk::new().file("games/Hades/save.dat");
        check(
            disk.path("games"),
            "Hades",
            Outcome::NotEmpty(disk.path("games/Hades")),
        );
    }

    #[test]
    fn refuses_a_location_that_is_a_file() {
        let disk = Disk::new().file("games");
        check(
            disk.path("games"),
            "Hades",
            Outcome::NotADirectory(disk.path("games")),
        );
    }

    #[test]
    fn refuses_a_location_nested_under_a_file() {
        let disk = Disk::new().file("games");
        check(
            disk.path("games/library"),
            "Hades",
            Outcome::NotADirectory(disk.path("games")),
        );
    }

    #[test]
    fn refuses_a_game_folder_that_is_a_file() {
        let disk = Disk::new().file("games/Hades");
        check(
            disk.path("games"),
            "Hades",
            Outcome::NotADirectory(disk.path("games/Hades")),
        );
    }
}
