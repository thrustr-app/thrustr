use super::sys::{At, write_at};
use bytes::Bytes;
use domain::download::DownloadReporter;
use std::{
    collections::{HashMap, VecDeque},
    fs::File,
    io, mem,
    sync::{Arc, Mutex, PoisonError, Weak},
};
use tokio::sync::watch;

pub const MAX_MERGED: usize = 1024 * 1024;
pub const MAX_QUEUED_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_QUEUED_WRITES: usize = 4096;

/// Writes to disk in order and in the background, while the plugin keeps going.
#[derive(Clone)]
pub struct Writer(Arc<Shared>);

struct Shared {
    /// By file address, which the weak refs keep from being reused.
    ids: Mutex<HashMap<usize, (Weak<File>, FileId)>>,
    reporter: Option<Arc<dyn DownloadReporter>>,
    queue: watch::Sender<Queue>,
}

impl Writer {
    pub fn new(reporter: Option<Arc<dyn DownloadReporter>>) -> Self {
        Self(Arc::new(Shared {
            ids: Mutex::default(),
            reporter,
            queue: watch::Sender::default(),
        }))
    }

    pub fn id_of(&self, file: &Arc<File>) -> io::Result<FileId> {
        let mut ids = self.0.ids.lock().unwrap_or_else(PoisonError::into_inner);
        let key = Arc::as_ptr(file) as usize;
        if let Some(&(_, id)) = ids.get(&key) {
            return Ok(id);
        }
        let id = FileId::of(file)?;
        ids.retain(|_, (file, _)| file.strong_count() > 0);
        ids.insert(key, (Arc::downgrade(file), id));
        Ok(id)
    }

    pub fn check(&self) -> io::Result<()> {
        self.0.queue.borrow().check()
    }

    pub fn is_running(&self) -> bool {
        self.0.queue.borrow().running
    }

    pub fn room(&self) -> usize {
        self.0.queue.borrow().room()
    }

    pub fn has_pending(&self, file: &Arc<File>) -> bool {
        if !self.is_running() {
            return false;
        }
        match self.id_of(file) {
            Ok(id) => self.0.queue.borrow().has_pending(id),
            Err(_) => true,
        }
    }

    pub async fn writable(&self) -> io::Result<()> {
        self.wait_for(|q| q.error.is_some() || q.room() > 0).await
    }

    /// Waits for `file`'s writes, or for all of them if the file can't be identified.
    pub async fn drain_file(&self, file: &Arc<File>) -> io::Result<()> {
        if !self.is_running() {
            return self.check();
        }
        let Ok(id) = self.id_of(file) else {
            return self.drain().await;
        };
        self.wait_for(|q| !q.has_pending(id)).await
    }

    pub async fn drain(&self) -> io::Result<()> {
        self.wait_for(|q| !q.running).await
    }

    async fn wait_for(&self, done: impl FnMut(&Queue) -> bool) -> io::Result<()> {
        let mut queue = self.0.queue.subscribe();
        let queue = queue
            .wait_for(done)
            .await
            .expect("the writer should hold the queue's sender");
        queue.check()
    }

    /// Drops the writes that haven't started.
    pub fn abandon(&self) {
        self.0.queue.send_modify(|queue| {
            let dropped: usize = queue.writes.drain(..).map(|write| write.data.len()).sum();
            queue.queued_bytes -= dropped;
        });
    }

    pub fn push(&self, file: Arc<File>, id: FileId, at: At, data: Bytes) -> io::Result<()> {
        if data.is_empty() {
            return self.check();
        }
        let start = self.modify(|queue| -> io::Result<bool> {
            queue.check()?;
            at.check_fits(data.len())?;
            queue.queued_bytes += data.len();
            queue.writes.push_back(QueuedWrite { file, id, at, data });
            Ok(!mem::replace(&mut queue.running, true))
        })?;
        if start {
            let writer = self.clone();
            tokio::task::spawn_blocking(move || writer.write_queued());
        }
        Ok(())
    }

    fn modify<R>(&self, modify: impl FnOnce(&mut Queue) -> R) -> R {
        let mut result = None;
        self.0
            .queue
            .send_modify(|queue| result = Some(modify(queue)));
        result.expect("send_modify calls its closure")
    }

    fn write_queued(&self) {
        let _guard = FailOnPanic(self);
        let mut batch = Vec::new();
        let mut merged = Vec::new();
        let mut landed = None;
        loop {
            let len = self.modify(|queue| {
                if let Some((len, result)) = landed.take() {
                    queue.queued_bytes -= len;
                    if let Err(error) = result {
                        queue.fail(error);
                    }
                }
                let len = queue.take_batch(&mut batch);
                queue.running = !batch.is_empty();
                queue.in_flight = batch.first().map(|write| write.id);
                len
            });

            let result = match &batch[..] {
                [] => return,
                [write] => write_at(&write.file, write.at, &write.data),
                writes @ [first, ..] => {
                    merged.clear();
                    for write in writes {
                        merged.extend_from_slice(&write.data);
                    }
                    write_at(&first.file, first.at, &merged)
                }
            };
            // Closes unused files before waiters wake.
            batch.clear();
            if let (Ok(()), Some(reporter)) = (&result, &self.0.reporter) {
                reporter.written(len as u64);
            }
            landed = Some((len, result));
        }
    }
}

/// Fails the queue if the writer thread panics, preventing waiters from hanging.
struct FailOnPanic<'a>(&'a Writer);

impl Drop for FailOnPanic<'_> {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            return;
        }
        self.0.0.queue.send_modify(|queue| {
            queue.fail(io::Error::other("the writer thread panicked"));
            queue.in_flight = None;
            queue.running = false;
        });
    }
}

#[derive(Default)]
struct Queue {
    writes: VecDeque<QueuedWrite>,
    queued_bytes: usize,
    /// The writer thread is running, even if it has nothing left to write.
    running: bool,
    in_flight: Option<FileId>,
    /// The first failed write. Every later write fails too.
    error: Option<io::Error>,
}

struct QueuedWrite {
    file: Arc<File>,
    id: FileId,
    at: At,
    data: Bytes,
}

impl Queue {
    fn has_pending(&self, id: FileId) -> bool {
        self.in_flight == Some(id) || self.writes.iter().any(|write| write.id == id)
    }

    fn room(&self) -> usize {
        if self.writes.len() >= MAX_QUEUED_WRITES {
            return 0;
        }
        MAX_QUEUED_BYTES.saturating_sub(self.queued_bytes)
    }

    fn check(&self) -> io::Result<()> {
        // Wasmtime picks the plugin's error code by OS error first.
        match &self.error {
            Some(error) => Err(match error.raw_os_error() {
                Some(code) => io::Error::from_raw_os_error(code),
                None => io::Error::new(error.kind(), error.to_string()),
            }),
            None => Ok(()),
        }
    }

    fn fail(&mut self, error: io::Error) {
        self.error.get_or_insert(error);
        self.writes.clear();
        self.queued_bytes = 0;
    }

    /// Moves the next write into the empty `batch`, along with the writes right
    /// after it that continue it in the same file, and returns the batch's length
    /// in bytes.
    fn take_batch(&mut self, batch: &mut Vec<QueuedWrite>) -> usize {
        debug_assert!(batch.is_empty());
        let Some(first) = self.writes.pop_front() else {
            return 0;
        };
        let mut len = first.data.len();
        batch.push(first);
        while let Some(next) = self.writes.front()
            && Arc::ptr_eq(&next.file, &batch[0].file)
            && next.at == batch[0].at.after(len)
            && len + next.data.len() <= MAX_MERGED
        {
            len += next.data.len();
            batch.extend(self.writes.pop_front());
        }
        len
    }
}

/// Identifies an open file across descriptors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileId {
    device: u64,
    index: u64,
}

impl FileId {
    #[cfg(unix)]
    fn of(file: &File) -> io::Result<Self> {
        use std::os::unix::fs::MetadataExt;
        let metadata = file.metadata()?;
        Ok(Self {
            device: metadata.dev(),
            index: metadata.ino(),
        })
    }

    #[cfg(windows)]
    fn of(file: &File) -> io::Result<Self> {
        let info = winapi_util::file::information(file)?;
        Ok(Self {
            device: info.volume_serial_number(),
            index: info.file_index(),
        })
    }
}

#[cfg(test)]
impl Writer {
    /// Bytes queued or in flight.
    pub fn queued_bytes(&self) -> usize {
        self.0.queue.borrow().queued_bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::host::fs::tests::open_rw;
    use domain::download::DownloadPhase;
    use std::time::Duration;
    use tempfile::TempDir;

    #[tokio::test]
    async fn a_panicking_writer_thread_fails_the_queue() {
        struct Panics;

        impl DownloadReporter for Panics {
            fn report(&self, _phase: DownloadPhase) {}

            fn received(&self, _bytes: u64) {}

            fn written(&self, _bytes: u64) {
                panic!("reporter panicked");
            }
        }

        let dir = TempDir::new().unwrap();
        let file = open_rw(&dir.path().join("game.bin"));
        let writer = Writer::new(Some(Arc::new(Panics)));
        let push = |data: &'static str| {
            let id = writer.id_of(&file).unwrap();
            writer.push(file.clone(), id, At::End, data.into())
        };
        push("ab").unwrap();

        let drain = tokio::time::timeout(Duration::from_secs(5), writer.drain());
        assert!(drain.await.expect("draining shouldn't hang").is_err());
        assert!(push("cd").is_err());
    }

    #[test]
    fn the_ids_of_closed_files_are_forgotten() {
        let dir = TempDir::new().unwrap();
        let writer = Writer::new(None);
        for i in 0..100 {
            let file = open_rw(&dir.path().join(i.to_string()));
            let id = writer.id_of(&file).unwrap();
            assert_eq!(id, FileId::of(&file).unwrap(), "file {i}");
        }
        assert!(writer.0.ids.lock().unwrap().len() <= 1);
    }
}
