use directories::ProjectDirs;
use std::{fs, path::PathBuf, sync::OnceLock};

static PROJECT_DIRS: OnceLock<ProjectDirs> = OnceLock::new();
const DB_NAME: &str = "thrustr.db";

/// Returns the path to the application data directory, creating it if it doesn't exist.
pub fn data_dir() -> PathBuf {
    let dir = project_dirs().data_dir().to_path_buf();
    fs::create_dir_all(&dir).expect("failed to create application data directory");
    dir
}

/// Returns the path to the application's database file.
pub fn db_path() -> PathBuf {
    data_dir().join(DB_NAME)
}

fn project_dirs() -> &'static ProjectDirs {
    PROJECT_DIRS.get_or_init(|| {
        ProjectDirs::from("com", "thrustr", "thrustr")
            .expect("failed to determine project directories")
    })
}
