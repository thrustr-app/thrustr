use directories::ProjectDirs;
use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

static PROJECT_DIRS: OnceLock<ProjectDirs> = OnceLock::new();
const DB_NAME: &str = "thrustr.db";

pub fn data_dir() -> &'static Path {
    project_dirs().data_dir()
}

pub fn db_path() -> PathBuf {
    data_dir().join("database").join(DB_NAME)
}

pub fn logs_dir() -> PathBuf {
    data_dir().join("logs")
}

pub fn artwork_dir() -> PathBuf {
    data_dir().join("artwork")
}

pub fn artwork_path(hash: &str, extension: &str) -> PathBuf {
    artwork_path_in(&artwork_dir(), hash, extension)
}

pub fn artwork_path_in(dir: &Path, hash: &str, extension: &str) -> PathBuf {
    let mut path = dir.to_path_buf();
    path.push(&hash[0..2]);
    path.push(&hash[2..4]);
    path.push(hash);
    path.set_extension(extension);
    path
}

pub fn plugins_dir() -> PathBuf {
    if cfg!(debug_assertions) {
        workspace_dir().join("target").join("plugins")
    } else {
        data_dir().join("plugins")
    }
}

pub fn cache_dir() -> PathBuf {
    if cfg!(debug_assertions) {
        workspace_dir().join("target").join("cache")
    } else {
        project_dirs().cache_dir().to_path_buf()
    }
}

pub fn plugins_cache_dir() -> PathBuf {
    cache_dir().join("plugins")
}

fn project_dirs() -> &'static ProjectDirs {
    PROJECT_DIRS.get_or_init(|| {
        ProjectDirs::from("com", "thrustr", "thrustr")
            .expect("platform should provide a home directory")
    })
}

fn workspace_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("config crate should live two levels below the workspace root")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[track_caller]
    fn check_artwork_path(dir: &str, hash: &str, extension: &str, expected: &str) {
        assert_eq!(
            artwork_path_in(Path::new(dir), hash, extension),
            PathBuf::from(expected)
        );
    }

    #[test]
    fn stores_artwork_in_hash_prefix_directories() {
        check_artwork_path(
            "/artwork",
            "abcdef123456",
            "webp",
            "/artwork/ab/cd/abcdef123456.webp",
        );
    }

    #[test]
    fn uses_the_requested_extension() {
        check_artwork_path(
            "/artwork",
            "abcdef123456",
            "foo",
            "/artwork/ab/cd/abcdef123456.foo",
        );
    }

    #[test]
    fn preserves_the_base_directory() {
        check_artwork_path(
            "/data/library/artwork",
            "abcdef123456",
            "webp",
            "/data/library/artwork/ab/cd/abcdef123456.webp",
        );
    }
}
