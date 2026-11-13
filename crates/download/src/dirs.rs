use futures::FutureExt;
use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, PoisonError, Weak},
};
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

const SCRATCH_DIR: &str = ".thrustr";

#[derive(Default)]
pub(crate) struct DirLocks(Mutex<HashMap<PathBuf, Weak<AsyncMutex<()>>>>);

pub(crate) type DirLock = OwnedMutexGuard<()>;

impl DirLocks {
    pub(crate) fn lock(&self, dir: &Path) -> impl Future<Output = DirLock> + use<> {
        let mutex = {
            let mut map = self.map();
            map.retain(|_, mutex| mutex.strong_count() > 0);
            match map.get(dir).and_then(Weak::upgrade) {
                Some(mutex) => mutex,
                None => {
                    let mutex = Arc::default();
                    map.insert(dir.to_path_buf(), Arc::downgrade(&mutex));
                    mutex
                }
            }
        };
        // Tokio queues a waiter on its first poll.
        let mut acquire = Box::pin(mutex.lock_owned());
        let ready = acquire.as_mut().now_or_never();
        async move {
            match ready {
                Some(lock) => lock,
                None => acquire.await,
            }
        }
    }

    fn map(&self) -> MutexGuard<'_, HashMap<PathBuf, Weak<AsyncMutex<()>>>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

pub(crate) fn remove_scratch(dir: &Path) -> io::Result<()> {
    remove_dir(&dir.join(SCRATCH_DIR))
}

pub(crate) fn remove_dir(dir: &Path) -> io::Result<()> {
    match fs::remove_dir_all(dir) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::pin::pin;
    use tempfile::TempDir;

    fn lock_now(locks: &DirLocks, dir: &str) -> Option<DirLock> {
        locks.lock(Path::new(dir)).now_or_never()
    }

    #[track_caller]
    fn check_waits(held: &[&str], dir: &str, waits: bool) {
        let locks = DirLocks::default();
        let guards: Vec<_> = held.iter().map(|d| lock_now(&locks, d).unwrap()).collect();

        let mut lock = pin!(locks.lock(Path::new(dir)));
        let waited = lock.as_mut().now_or_never().is_none();
        assert_eq!(waited, waits, "locking {dir} while {held:?} are locked");
        if waited {
            drop(guards);
            assert!(lock.now_or_never().is_some(), "{dir} stayed locked");
        }
    }

    #[test]
    fn a_dir_waits_only_for_its_own_lock() {
        check_waits(&[], "a", false);
        check_waits(&["a"], "a", true);
        check_waits(&["a"], "b", false);
        check_waits(&["b", "a", "c"], "a", true);
    }

    #[test]
    fn locks_are_granted_in_call_order() {
        let locks = DirLocks::default();
        let held = lock_now(&locks, "a").unwrap();
        let first = pin!(locks.lock(Path::new("a")));
        let mut second = pin!(locks.lock(Path::new("a")));
        assert!(second.as_mut().now_or_never().is_none());

        drop(held);
        assert!(
            second.now_or_never().is_none(),
            "the second lock went first"
        );
        assert!(
            first.now_or_never().is_some(),
            "the first lock stayed locked"
        );
    }

    #[test]
    fn locks_nobody_holds_or_waits_for_are_forgotten() {
        let locks = DirLocks::default();
        drop(lock_now(&locks, "released"));
        let held = lock_now(&locks, "stopped").unwrap();
        {
            let mut stopped = pin!(locks.lock(Path::new("stopped")));
            assert!(stopped.as_mut().now_or_never().is_none());
            drop(held);
        }

        let _held = lock_now(&locks, "held");
        let remembered: Vec<_> = locks.map().keys().cloned().collect();
        assert_eq!(remembered, [Path::new("held")]);
    }

    #[track_caller]
    fn check_removal(files: &[&str], remove: fn(&Path) -> io::Result<()>, expected: &[&str]) {
        let root = TempDir::new().unwrap();
        let dir = root.path().join("game");
        for file in files {
            let path = dir.join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"x").unwrap();
        }

        remove(&dir).unwrap();

        let mut left = Vec::new();
        list(root.path(), root.path(), &mut left);
        left.sort();
        assert_eq!(left, expected, "{files:?}");
    }

    fn list(root: &Path, dir: &Path, files: &mut Vec<String>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                list(root, &path, files);
            } else {
                let relative = path.strip_prefix(root).unwrap();
                files.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }

    #[test]
    fn a_finished_download_keeps_only_the_game() {
        check_removal(
            &[
                "game.exe",
                "data/level.pak",
                ".thrustr/state",
                ".thrustr/staging/setup.zip",
            ],
            remove_scratch,
            &["game/data/level.pak", "game/game.exe"],
        );
        check_removal(&["game.exe"], remove_scratch, &["game/game.exe"]);
        check_removal(&[], remove_scratch, &[]);
    }

    #[test]
    fn a_cancelled_download_leaves_nothing() {
        check_removal(&["game.exe", ".thrustr/state"], remove_dir, &[]);
        check_removal(&[], remove_dir, &[]);
    }
}
