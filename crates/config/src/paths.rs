use directories::ProjectDirs;
use std::{fs, path::PathBuf, process::Command, sync::OnceLock};

static PROJECT_DIRS: OnceLock<ProjectDirs> = OnceLock::new();
const DB_NAME: &str = "thrustr.db";

/// Path to the application data directory, creating it if it doesn't exist.
pub fn data_dir() -> PathBuf {
    let dir = project_dirs().data_dir().to_path_buf();
    fs::create_dir_all(&dir).expect("Failed to create application data directory");
    dir
}

/// Path to the application database.
pub fn db_path() -> PathBuf {
    let dir = data_dir().join("database");
    fs::create_dir_all(&dir).expect("Failed to create application database directory");
    dir.join(DB_NAME)
}

/// Path to the artwork images directory.
pub fn artwork_dir() -> PathBuf {
    data_dir().join("artwork")
}

/// Path to the application logs directory, creating it if it doesn't exist.
pub fn logs_dir() -> PathBuf {
    let dir = data_dir().join("logs");
    fs::create_dir_all(&dir).expect("Failed to create application logs directory");
    dir
}

/// Path to the artwork image for a given content hash.
pub fn artwork_path(hash: &str, extension: &str) -> PathBuf {
    artwork_dir()
        .join(&hash[0..2])
        .join(&hash[2..4])
        .join(format!("{hash}.{extension}"))
}

/// Path to the plugins directory, creating it if it doesn't exist.
pub fn plugins_dir() -> PathBuf {
    let dir = if cfg!(debug_assertions) {
        workspace_dir().join("target").join("plugins")
    } else {
        data_dir().join("plugins")
    };
    fs::create_dir_all(&dir).expect("Failed to create plugins directory");
    dir
}

/// Path to the application cache directory, creating it if it doesn't exist.
pub fn cache_dir() -> PathBuf {
    let dir = if cfg!(debug_assertions) {
        workspace_dir().join("target").join("cache")
    } else {
        project_dirs().cache_dir().to_path_buf()
    };
    fs::create_dir_all(&dir).expect("Failed to create application cache directory");
    dir
}

/// Path to the compiled-plugin cache directory, creating it if it doesn't exist.
pub fn plugins_cache_dir() -> PathBuf {
    let dir = cache_dir().join("plugins");
    fs::create_dir_all(&dir).expect("Failed to create plugins cache directory");
    dir
}

fn project_dirs() -> &'static ProjectDirs {
    PROJECT_DIRS.get_or_init(|| {
        ProjectDirs::from("com", "thrustr", "thrustr")
            .expect("Failed to determine project directories")
    })
}

fn workspace_dir() -> PathBuf {
    let output = Command::new(env!("CARGO"))
        .args(["locate-project", "--workspace", "--message-format=plain"])
        .output()
        .expect("Failed to run cargo locate-project");

    let cargo_toml = PathBuf::from(
        String::from_utf8(output.stdout)
            .expect("Failed to parse cargo locate-project output")
            .trim(),
    );

    cargo_toml
        .parent()
        .expect("Cargo.toml has no parent directory")
        .to_path_buf()
}
