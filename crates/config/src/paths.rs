use directories::ProjectDirs;
use std::{fs, path::PathBuf, process::Command, sync::OnceLock};

static PROJECT_DIRS: OnceLock<ProjectDirs> = OnceLock::new();
const DB_NAME: &str = "thrustr.db";

/// Path to the application data directory, creating it if it doesn't exist.
pub fn data_dir() -> PathBuf {
    let dir = project_dirs().data_dir().to_path_buf();
    fs::create_dir_all(&dir).expect("failed to create application data directory");
    dir
}

/// Path to the application database.
pub fn db_path() -> PathBuf {
    data_dir().join(DB_NAME)
}

/// Path to the applicationmedia directory.
pub fn media_dir() -> PathBuf {
    data_dir().join("media")
}

/// Path to the cover images directoy.
pub fn covers_dir() -> PathBuf {
    media_dir().join("covers")
}

/// Path to the cover image for a given ID.
pub fn cover_path(id: u64, extension: &str) -> PathBuf {
    let base = covers_dir();

    let dir1 = id / 1_000;
    let dir2 = id % 1_000;

    base.join(format!("{dir1:03}"))
        .join(format!("{dir2:03}"))
        .join(format!("{id}.{extension}"))
}

/// Path to the plugins directory, creating it if it doesn't exist.
pub fn plugins_dir() -> PathBuf {
    let dir = if cfg!(debug_assertions) {
        workspace_dir().join("target").join("plugins")
    } else {
        data_dir().join("plugins")
    };
    fs::create_dir_all(&dir).expect("failed to create plugins directory");
    dir
}

fn project_dirs() -> &'static ProjectDirs {
    PROJECT_DIRS.get_or_init(|| {
        ProjectDirs::from("com", "thrustr", "thrustr")
            .expect("failed to determine project directories")
    })
}

fn workspace_dir() -> PathBuf {
    let output = Command::new(env!("CARGO"))
        .args(["locate-project", "--workspace", "--message-format=plain"])
        .output()
        .expect("failed to run cargo locate-project");

    let cargo_toml = PathBuf::from(
        String::from_utf8(output.stdout)
            .expect("failed to parse cargo locate-project output")
            .trim(),
    );

    cargo_toml
        .parent()
        .expect("Cargo.toml has no parent directory")
        .to_path_buf()
}
