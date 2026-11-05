use crate::plugin::PluginState;
use async_trait::async_trait;
use bytes::{Bytes, BytesMut};
use domain::download::DownloadReporter;
use std::{
    collections::{HashMap, VecDeque},
    fs::File,
    io, mem,
    sync::{Arc, Mutex, PoisonError, Weak},
};
use tokio::sync::watch;
use wasmtime::component::{HasData, Linker, Resource};
use wasmtime_wasi::{
    OpenMode,
    filesystem::{Descriptor, WasiFilesystemCtxView},
    p2::{
        DynInputStream, DynOutputStream, FsError, FsResult, InputStream, OutputStream, Pollable,
        StreamError, StreamResult,
        bindings::filesystem::types::{
            self, Advice, DescriptorFlags, DescriptorStat, DescriptorType, DirectoryEntry,
            DirectoryEntryStream, ErrorCode, Filesize, HostDescriptor, HostDirectoryEntryStream,
            MetadataHashValue, NewTimestamp, OpenFlags, PathFlags,
        },
    },
};

/// Largest write a stream accepts at once. Wasmtime copies each write into a
/// fresh buffer, and keeping them this small lets the allocator reuse them,
/// which is faster on Windows.
const MAX_GRANT: usize = 256 * 1024;
const MAX_MERGED: usize = 1024 * 1024;
const MAX_QUEUED: usize = 8 * 1024 * 1024;
const MAX_QUEUED_WRITES: usize = 4096;
/// Largest read a stream returns at once, like Wasmtime.
const MAX_READ: usize = 64 * 1024;

pub(crate) fn add_to_linker(linker: &mut Linker<PluginState>) -> wasmtime::Result<()> {
    linker.allow_shadowing(true);
    types::add_to_linker::<PluginState, Fs>(linker, PluginState::fs)?;
    linker.allow_shadowing(false);
    Ok(())
}

pub(crate) struct Fs;

impl HasData for Fs {
    type Data<'a> = FsView<'a>;
}

pub(crate) struct FsView<'a> {
    fs: WasiFilesystemCtxView<'a>,
    writer: &'a Writer,
}

impl<'a> FsView<'a> {
    pub(crate) fn new(fs: WasiFilesystemCtxView<'a>, writer: &'a Writer) -> Self {
        Self { fs, writer }
    }

    async fn drain_file(&mut self, fd: &Resource<Descriptor>) -> FsResult<()> {
        let file = match self.fs.table.get(fd)? {
            Descriptor::File(file) => file.file.clone(),
            Descriptor::Dir(_) => return Ok(self.writer.check()?),
        };
        Ok(self.writer.drain_file(&file).await?)
    }

    /// Waits for all queued writes, unless `path` is a directory or doesn't exist.
    async fn drain_path(
        &mut self,
        fd: &Resource<Descriptor>,
        path_flags: PathFlags,
        path: &str,
    ) -> FsResult<()> {
        if self.writer.is_running() && self.may_have_writes(fd, path_flags, path).await {
            return Ok(self.writer.drain().await?);
        }
        Ok(self.writer.check()?)
    }

    async fn may_have_writes(
        &mut self,
        fd: &Resource<Descriptor>,
        path_flags: PathFlags,
        path: &str,
    ) -> bool {
        let stat = HostDescriptor::stat_at(&mut self.fs, borrow(fd), path_flags, path.into());
        match stat.await {
            Ok(stat) => stat.type_ != DescriptorType::Directory,
            Err(error) => error.downcast_ref() != Some(&ErrorCode::NoEntry),
        }
    }

    fn writable_file(&self, fd: &Resource<Descriptor>) -> FsResult<(Arc<File>, FileId)> {
        let file = match self.fs.table.get(fd)? {
            Descriptor::File(file) if file.open_mode.contains(OpenMode::WRITE) => file.file.clone(),
            _ => return Err(ErrorCode::BadDescriptor.into()),
        };
        let id = self.writer.id_of(&file)?;
        Ok((file, id))
    }

    fn queued_output(
        &mut self,
        fd: Resource<Descriptor>,
        at: At,
    ) -> FsResult<Resource<DynOutputStream>> {
        let (file, id) = self.writable_file(&fd)?;
        let stream: DynOutputStream = Box::new(QueuedOutput {
            writer: self.writer.clone(),
            file,
            id,
            at,
            granted: 0,
        });
        Ok(self.fs.table.push(stream)?)
    }
}

impl types::Host for FsView<'_> {
    fn convert_error_code(&mut self, err: FsError) -> wasmtime::Result<ErrorCode> {
        types::Host::convert_error_code(&mut self.fs, err)
    }

    fn filesystem_error_code(
        &mut self,
        err: Resource<wasmtime::Error>,
    ) -> wasmtime::Result<Option<ErrorCode>> {
        types::Host::filesystem_error_code(&mut self.fs, err)
    }
}

impl HostDescriptor for FsView<'_> {
    async fn write(
        &mut self,
        fd: Resource<Descriptor>,
        buf: Vec<u8>,
        offset: Filesize,
    ) -> FsResult<Filesize> {
        let (file, id) = self.writable_file(&fd)?;
        self.writer.writable().await?;
        let len = buf.len() as Filesize;
        self.writer
            .push(file, id, At::Offset(offset), buf.into(), 0)?;
        Ok(len)
    }

    fn write_via_stream(
        &mut self,
        fd: Resource<Descriptor>,
        offset: Filesize,
    ) -> FsResult<Resource<DynOutputStream>> {
        self.queued_output(fd, At::Offset(offset))
    }

    fn append_via_stream(
        &mut self,
        fd: Resource<Descriptor>,
    ) -> FsResult<Resource<DynOutputStream>> {
        self.queued_output(fd, At::End)
    }

    fn read_via_stream(
        &mut self,
        fd: Resource<Descriptor>,
        offset: Filesize,
    ) -> FsResult<Resource<DynInputStream>> {
        let file = match self.fs.table.get(&fd)? {
            Descriptor::File(file) => file.file.clone(),
            Descriptor::Dir(_) => return HostDescriptor::read_via_stream(&mut self.fs, fd, offset),
        };
        let stream: DynInputStream = Box::new(QueuedInput {
            writer: self.writer.clone(),
            file,
            position: offset,
        });
        Ok(self.fs.table.push(stream)?)
    }

    fn drop(&mut self, fd: Resource<Descriptor>) -> wasmtime::Result<()> {
        HostDescriptor::drop(&mut self.fs, fd)
    }

    async fn advise(
        &mut self,
        fd: Resource<Descriptor>,
        offset: Filesize,
        len: Filesize,
        advice: Advice,
    ) -> FsResult<()> {
        HostDescriptor::advise(&mut self.fs, fd, offset, len, advice).await
    }

    async fn sync_data(&mut self, fd: Resource<Descriptor>) -> FsResult<()> {
        self.drain_file(&fd).await?;
        HostDescriptor::sync_data(&mut self.fs, fd).await
    }

    async fn get_flags(&mut self, fd: Resource<Descriptor>) -> FsResult<DescriptorFlags> {
        HostDescriptor::get_flags(&mut self.fs, fd).await
    }

    async fn get_type(&mut self, fd: Resource<Descriptor>) -> FsResult<DescriptorType> {
        HostDescriptor::get_type(&mut self.fs, fd).await
    }

    async fn set_size(&mut self, fd: Resource<Descriptor>, size: Filesize) -> FsResult<()> {
        self.drain_file(&fd).await?;
        HostDescriptor::set_size(&mut self.fs, fd, size).await
    }

    async fn set_times(
        &mut self,
        fd: Resource<Descriptor>,
        atim: NewTimestamp,
        mtim: NewTimestamp,
    ) -> FsResult<()> {
        self.drain_file(&fd).await?;
        HostDescriptor::set_times(&mut self.fs, fd, atim, mtim).await
    }

    async fn read(
        &mut self,
        fd: Resource<Descriptor>,
        len: Filesize,
        offset: Filesize,
    ) -> FsResult<(Vec<u8>, bool)> {
        self.drain_file(&fd).await?;
        HostDescriptor::read(&mut self.fs, fd, len, offset).await
    }

    async fn read_directory(
        &mut self,
        fd: Resource<Descriptor>,
    ) -> FsResult<Resource<DirectoryEntryStream>> {
        HostDescriptor::read_directory(&mut self.fs, fd).await
    }

    async fn sync(&mut self, fd: Resource<Descriptor>) -> FsResult<()> {
        self.drain_file(&fd).await?;
        HostDescriptor::sync(&mut self.fs, fd).await
    }

    async fn create_directory_at(
        &mut self,
        fd: Resource<Descriptor>,
        path: String,
    ) -> FsResult<()> {
        HostDescriptor::create_directory_at(&mut self.fs, fd, path).await
    }

    async fn stat(&mut self, fd: Resource<Descriptor>) -> FsResult<DescriptorStat> {
        self.drain_file(&fd).await?;
        HostDescriptor::stat(&mut self.fs, fd).await
    }

    async fn stat_at(
        &mut self,
        fd: Resource<Descriptor>,
        path_flags: PathFlags,
        path: String,
    ) -> FsResult<DescriptorStat> {
        self.drain_path(&fd, path_flags, &path).await?;
        HostDescriptor::stat_at(&mut self.fs, fd, path_flags, path).await
    }

    async fn set_times_at(
        &mut self,
        fd: Resource<Descriptor>,
        path_flags: PathFlags,
        path: String,
        atim: NewTimestamp,
        mtim: NewTimestamp,
    ) -> FsResult<()> {
        self.drain_path(&fd, path_flags, &path).await?;
        HostDescriptor::set_times_at(&mut self.fs, fd, path_flags, path, atim, mtim).await
    }

    async fn link_at(
        &mut self,
        fd: Resource<Descriptor>,
        old_path_flags: PathFlags,
        old_path: String,
        new_descriptor: Resource<Descriptor>,
        new_path: String,
    ) -> FsResult<()> {
        HostDescriptor::link_at(
            &mut self.fs,
            fd,
            old_path_flags,
            old_path,
            new_descriptor,
            new_path,
        )
        .await
    }

    async fn open_at(
        &mut self,
        fd: Resource<Descriptor>,
        path_flags: PathFlags,
        path: String,
        oflags: OpenFlags,
        flags: DescriptorFlags,
    ) -> FsResult<Resource<Descriptor>> {
        // Only truncating depends on previous writes.
        if oflags.contains(OpenFlags::TRUNCATE) {
            self.drain_path(&fd, path_flags, &path).await?;
        } else {
            self.writer.check()?;
        }
        HostDescriptor::open_at(&mut self.fs, fd, path_flags, path, oflags, flags).await
    }

    async fn readlink_at(&mut self, fd: Resource<Descriptor>, path: String) -> FsResult<String> {
        HostDescriptor::readlink_at(&mut self.fs, fd, path).await
    }

    async fn remove_directory_at(
        &mut self,
        fd: Resource<Descriptor>,
        path: String,
    ) -> FsResult<()> {
        HostDescriptor::remove_directory_at(&mut self.fs, fd, path).await
    }

    /// Windows refuses to rename files that queued writes hold open.
    async fn rename_at(
        &mut self,
        fd: Resource<Descriptor>,
        old_path: String,
        new_fd: Resource<Descriptor>,
        new_path: String,
    ) -> FsResult<()> {
        self.writer.drain().await?;
        HostDescriptor::rename_at(&mut self.fs, fd, old_path, new_fd, new_path).await
    }

    async fn symlink_at(
        &mut self,
        fd: Resource<Descriptor>,
        src_path: String,
        dest_path: String,
    ) -> FsResult<()> {
        HostDescriptor::symlink_at(&mut self.fs, fd, src_path, dest_path).await
    }

    /// Windows refuses to unlink files that queued writes hold open.
    async fn unlink_file_at(&mut self, fd: Resource<Descriptor>, path: String) -> FsResult<()> {
        self.writer.drain().await?;
        HostDescriptor::unlink_file_at(&mut self.fs, fd, path).await
    }

    async fn is_same_object(
        &mut self,
        a: Resource<Descriptor>,
        b: Resource<Descriptor>,
    ) -> wasmtime::Result<bool> {
        HostDescriptor::is_same_object(&mut self.fs, a, b).await
    }

    async fn metadata_hash(&mut self, fd: Resource<Descriptor>) -> FsResult<MetadataHashValue> {
        self.drain_file(&fd).await?;
        HostDescriptor::metadata_hash(&mut self.fs, fd).await
    }

    async fn metadata_hash_at(
        &mut self,
        fd: Resource<Descriptor>,
        path_flags: PathFlags,
        path: String,
    ) -> FsResult<MetadataHashValue> {
        self.drain_path(&fd, path_flags, &path).await?;
        HostDescriptor::metadata_hash_at(&mut self.fs, fd, path_flags, path).await
    }
}

impl HostDirectoryEntryStream for FsView<'_> {
    async fn read_directory_entry(
        &mut self,
        stream: Resource<DirectoryEntryStream>,
    ) -> FsResult<Option<DirectoryEntry>> {
        HostDirectoryEntryStream::read_directory_entry(&mut self.fs, stream).await
    }

    fn drop(&mut self, stream: Resource<DirectoryEntryStream>) -> wasmtime::Result<()> {
        HostDirectoryEntryStream::drop(&mut self.fs, stream)
    }
}

fn borrow<T: 'static>(resource: &Resource<T>) -> Resource<T> {
    Resource::new_borrow(resource.rep())
}

/// Identifies an open file across descriptors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileId {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum At {
    Offset(u64),
    End,
}

impl At {
    fn after(self, len: usize) -> At {
        match self {
            At::Offset(offset) => At::Offset(offset + len as u64),
            At::End => At::End,
        }
    }

    fn check_fits(self, len: usize) -> io::Result<()> {
        match self {
            At::Offset(offset) if offset > i64::MAX as u64 - len as u64 => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "write past the largest file offset",
            )),
            _ => Ok(()),
        }
    }
}

/// Writes to disk in order, in the background, while the plugin keeps going.
#[derive(Clone)]
pub(crate) struct Writer(Arc<Shared>);

struct Shared {
    /// By file address, which the weak refs keep from being reused.
    ids: Mutex<HashMap<usize, (Weak<File>, FileId)>>,
    reporter: Option<Arc<dyn DownloadReporter>>,
    queue: watch::Sender<Queue>,
}

#[derive(Default)]
struct Queue {
    writes: VecDeque<Write>,
    queued: usize,
    /// Granted to streams but not yet written.
    granted: usize,
    /// The writer thread is running, even if it has nothing left to write.
    running: bool,
    in_flight: Option<FileId>,
    /// The first failed write. Every later write fails too.
    error: Option<io::Error>,
}

struct Write {
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
        MAX_QUEUED.saturating_sub(self.queued + self.granted)
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
        self.queued = 0;
    }

    /// Moves the next write into the empty `batch`, along with the writes right
    /// after it that continue it in the same file, and returns the batch's length
    /// in bytes.
    fn take_batch(&mut self, batch: &mut Vec<Write>) -> usize {
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

impl Writer {
    pub(crate) fn new(reporter: Option<Arc<dyn DownloadReporter>>) -> Self {
        Self(Arc::new(Shared {
            ids: Mutex::default(),
            reporter,
            queue: watch::Sender::default(),
        }))
    }

    fn id_of(&self, file: &Arc<File>) -> io::Result<FileId> {
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

    fn check(&self) -> io::Result<()> {
        self.0.queue.borrow().check()
    }

    fn is_running(&self) -> bool {
        self.0.queue.borrow().running
    }

    fn room(&self) -> usize {
        self.0.queue.borrow().room()
    }

    fn has_pending(&self, file: &Arc<File>) -> bool {
        if !self.is_running() {
            return false;
        }
        match self.id_of(file) {
            Ok(id) => self.0.queue.borrow().has_pending(id),
            Err(_) => true,
        }
    }

    /// Replaces `granted` with a new grant and returns its size.
    fn grant(&self, granted: &mut usize) -> io::Result<usize> {
        self.modify(|queue| {
            queue.granted -= mem::take(granted);
            queue.check()?;
            *granted = MAX_GRANT.min(queue.room());
            queue.granted += *granted;
            Ok(*granted)
        })
    }

    fn release(&self, granted: usize) {
        if granted > 0 {
            self.0.queue.send_modify(|queue| queue.granted -= granted);
        }
    }

    async fn writable(&self) -> io::Result<()> {
        self.wait_for(|q| q.error.is_some() || q.room() > 0).await
    }

    /// Waits for `file`'s writes, or for all of them if the file can't be identified.
    async fn drain_file(&self, file: &Arc<File>) -> io::Result<()> {
        if !self.is_running() {
            return self.check();
        }
        let Ok(id) = self.id_of(file) else {
            return self.drain().await;
        };
        self.wait_for(|q| !q.has_pending(id)).await
    }

    pub(crate) async fn drain(&self) -> io::Result<()> {
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
    pub(crate) fn abandon(&self) {
        self.0.queue.send_modify(|queue| {
            let dropped: usize = queue.writes.drain(..).map(|write| write.data.len()).sum();
            queue.queued -= dropped;
        });
    }

    /// Queues `data`, using up `granted`.
    fn push(
        &self,
        file: Arc<File>,
        id: FileId,
        at: At,
        data: Bytes,
        granted: usize,
    ) -> io::Result<()> {
        if data.is_empty() {
            self.release(granted);
            return self.check();
        }
        let start = self.modify(|queue| -> io::Result<bool> {
            queue.granted -= granted;
            queue.check()?;
            at.check_fits(data.len())?;
            queue.queued += data.len();
            queue.writes.push_back(Write { file, id, at, data });
            Ok(!mem::replace(&mut queue.running, true))
        })?;
        if start {
            let writer = self.clone();
            tokio::task::spawn_blocking(move || writer.write_queued());
        }
        Ok(())
    }

    /// Like `send_modify`, but returns what `modify` returns.
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
                    queue.queued -= len;
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

fn write_at(file: &File, at: At, data: &[u8]) -> io::Result<()> {
    let offset = match at {
        At::Offset(offset) => offset,
        At::End => file.metadata()?.len(),
    };
    write_all_at(file, data, offset)
}

#[cfg(unix)]
fn write_all_at(file: &File, data: &[u8], offset: u64) -> io::Result<()> {
    use std::os::unix::fs::FileExt;
    file.write_all_at(data, offset)
}

#[cfg(windows)]
fn write_all_at(file: &File, mut data: &[u8], mut offset: u64) -> io::Result<()> {
    use std::os::windows::fs::FileExt;
    while !data.is_empty() {
        match file.seek_write(data, offset)? {
            0 => return Err(io::ErrorKind::WriteZero.into()),
            written => {
                data = &data[written..];
                offset += written as u64;
            }
        }
    }
    Ok(())
}

struct QueuedOutput {
    writer: Writer,
    file: Arc<File>,
    id: FileId,
    at: At,
    /// Reserved by the last `check_write`.
    granted: usize,
}

#[async_trait]
impl Pollable for QueuedOutput {
    async fn ready(&mut self) {
        if self.granted == 0 {
            // The next `check_write` reports a failed write.
            let _ = self.writer.writable().await;
        }
    }
}

#[async_trait]
impl OutputStream for QueuedOutput {
    /// Traps beyond the grant, as wasmtime doesn't enforce it.
    fn write(&mut self, bytes: Bytes) -> StreamResult<()> {
        let len = bytes.len();
        if len > self.granted {
            return Err(StreamError::trap("write exceeded budget"));
        }
        let granted = mem::take(&mut self.granted);
        self.writer
            .push(self.file.clone(), self.id, self.at, bytes, granted)
            .map_err(stream_error)?;
        self.at = self.at.after(len);
        Ok(())
    }

    /// Queued counts as flushed, like OS write-back.
    fn flush(&mut self) -> StreamResult<()> {
        self.writer.check().map_err(stream_error)
    }

    fn check_write(&mut self) -> StreamResult<usize> {
        self.writer.grant(&mut self.granted).map_err(stream_error)
    }

    /// Gives back this stream's grant and waits for room without reserving it,
    /// since `blocking-flush` calls this last and idle streams shouldn't hold
    /// room. Writes still go through `check_write`.
    async fn write_ready(&mut self) -> StreamResult<usize> {
        self.writer.release(mem::take(&mut self.granted));
        self.writer.writable().await.map_err(stream_error)?;
        Ok(MAX_GRANT.min(self.writer.room()))
    }

    /// The default writes the amount returned by `write_ready`, but that
    /// amount isn't reserved here.
    async fn blocking_write_and_flush(&mut self, mut bytes: Bytes) -> StreamResult<()> {
        while !bytes.is_empty() {
            self.ready().await;
            let permit = self.check_write()?;
            self.write(bytes.split_to(permit.min(bytes.len())))?;
        }
        self.flush()
    }
}

impl Drop for QueuedOutput {
    fn drop(&mut self) {
        self.writer.release(self.granted);
    }
}

/// Each read waits for the file's earlier writes, including those queued after
/// the stream was opened. Unlike Wasmtime's stream, reads run on the calling
/// task, so none outlives the call, and nothing is buffered, so a read never
/// returns data that a write has since replaced.
struct QueuedInput {
    writer: Writer,
    file: Arc<File>,
    position: u64,
}

impl QueuedInput {
    fn read_now(&mut self, size: usize) -> StreamResult<Bytes> {
        self.writer.check().map_err(stream_error)?;
        let mut buf = BytesMut::zeroed(size.min(MAX_READ));
        let len = read_at(&self.file, &mut buf, self.position).map_err(stream_error)?;
        if len == 0 && !buf.is_empty() {
            return Err(StreamError::Closed);
        }
        buf.truncate(len);
        self.position += len as u64;
        Ok(buf.freeze())
    }
}

#[async_trait]
impl Pollable for QueuedInput {
    async fn ready(&mut self) {
        // The next read reports a failed write.
        let _ = self.writer.drain_file(&self.file).await;
    }
}

#[async_trait]
impl InputStream for QueuedInput {
    fn read(&mut self, size: usize) -> StreamResult<Bytes> {
        if self.writer.has_pending(&self.file) {
            return Ok(Bytes::new());
        }
        self.read_now(size)
    }

    async fn blocking_read(&mut self, size: usize) -> StreamResult<Bytes> {
        self.writer
            .drain_file(&self.file)
            .await
            .map_err(stream_error)?;
        self.read_now(size)
    }
}

#[cfg(unix)]
fn read_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<usize> {
    use std::os::unix::fs::FileExt;
    loop {
        match file.read_at(buf, offset) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            result => return result,
        }
    }
}

#[cfg(windows)]
fn read_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<usize> {
    use std::os::windows::fs::FileExt;
    file.seek_read(buf, offset)
}

fn stream_error(error: io::Error) -> StreamError {
    StreamError::LastOperationFailed(error.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::download::DownloadPhase;
    use futures::FutureExt;
    use rand::{RngExt, SeedableRng, rngs::StdRng};
    use std::{
        collections::BTreeMap,
        fmt::Debug,
        fs::{self, OpenOptions},
        path::{Path, PathBuf},
        pin::pin,
        sync::{
            Condvar, MutexGuard,
            atomic::{AtomicU64, Ordering},
        },
        time::Duration,
    };
    use tempfile::TempDir;
    use wasmtime_wasi::{
        FsPerms, ResourceTable, WasiCtx, WasiCtxView, WasiView,
        filesystem::WasiFilesystemView,
        p2::bindings::{filesystem::preopens, io::streams},
    };

    /// Tracks how many bytes are written and, while paused, holds the writer thread
    /// after each write, so the queue still sees it in flight, like a slow
    /// disk.
    #[derive(Default)]
    struct Disk {
        landed: AtomicU64,
        state: Mutex<DiskState>,
        changed: Condvar,
    }

    #[derive(Default)]
    struct DiskState {
        paused: bool,
        holding: bool,
    }

    impl Disk {
        fn pause(self: &Arc<Self>) -> Paused {
            self.lock().paused = true;
            Paused(self.clone())
        }

        fn wait_until_holding(&self) {
            let state = self.lock();
            let _state = self.changed.wait_while(state, |state| !state.holding);
        }

        fn landed(&self) -> u64 {
            self.landed.load(Ordering::SeqCst)
        }

        fn lock(&self) -> MutexGuard<'_, DiskState> {
            self.state.lock().unwrap_or_else(PoisonError::into_inner)
        }
    }

    impl DownloadReporter for Disk {
        fn progress(&self, _phase: DownloadPhase, _processed: u64, _total: Option<u64>) {}

        fn received(&self, _bytes: u64) {}

        fn written(&self, bytes: u64) {
            self.landed.fetch_add(bytes, Ordering::SeqCst);
            let mut state = self.lock();
            state.holding = true;
            self.changed.notify_all();
            let mut state = self
                .changed
                .wait_while(state, |state| state.paused)
                .unwrap_or_else(PoisonError::into_inner);
            state.holding = false;
        }
    }

    /// Resumes the disk when dropped.
    struct Paused(Arc<Disk>);

    impl Drop for Paused {
        fn drop(&mut self) {
            self.0.lock().paused = false;
            self.0.changed.notify_all();
        }
    }

    fn writer() -> (Writer, Arc<Disk>) {
        let disk = Arc::new(Disk::default());
        (Writer::new(Some(disk.clone())), disk)
    }

    fn open(dir: &Path, name: &str) -> (Arc<File>, FileId) {
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(dir.join(name))
            .unwrap();
        let id = FileId::of(&file).unwrap();
        (Arc::new(file), id)
    }

    fn reserved(writer: &Writer) -> usize {
        let queue = writer.0.queue.borrow();
        queue.queued + queue.granted
    }

    async fn check_writes(writes: &[(At, &str)], expected: &str) {
        let dir = TempDir::new().unwrap();
        let (file, id) = open(dir.path(), "game.bin");
        let (writer, disk) = writer();
        for &(at, data) in writes {
            let data = Bytes::copy_from_slice(data.as_bytes());
            writer.push(file.clone(), id, at, data, 0).unwrap();
        }

        writer.drain().await.unwrap();
        let contents = fs::read_to_string(dir.path().join("game.bin")).unwrap();
        assert_eq!(contents, expected, "{writes:?}");
        let len: u64 = writes.iter().map(|(_, data)| data.len() as u64).sum();
        assert_eq!(disk.landed(), len, "{writes:?}");
    }

    #[tokio::test]
    async fn queued_writes_land_in_order() {
        use At::*;
        check_writes(&[(Offset(0), "aaaa"), (Offset(1), "bb")], "abba").await;
        check_writes(&[(Offset(2), "cd"), (Offset(0), "ab")], "abcd").await;
        check_writes(&[(End, "ab"), (End, "cd")], "abcd").await;
        check_writes(&[(Offset(0), "ab"), (End, "cd"), (Offset(1), "x")], "axcd").await;
        check_writes(&[(Offset(3), "x")], "\0\0\0x").await;
    }

    #[track_caller]
    fn check_merged(writes: &[(usize, At, usize)], expected: &[(usize, At, usize)]) {
        let dir = TempDir::new().unwrap();
        let files = [open(dir.path(), "a"), open(dir.path(), "b")];
        let mut queue = Queue::default();
        for &(file, at, len) in writes {
            let (file, id) = files[file].clone();
            let data = Bytes::from(vec![0; len]);
            queue.writes.push_back(Write { file, id, at, data });
        }

        let merged: Vec<_> = std::iter::from_fn(|| {
            let mut batch = Vec::new();
            let len = queue.take_batch(&mut batch);
            let first = batch.first()?;
            let file = files.iter().position(|(f, _)| Arc::ptr_eq(f, &first.file));
            assert_eq!(
                len,
                batch.iter().map(|write| write.data.len()).sum::<usize>()
            );
            Some((file.unwrap(), first.at, len))
        })
        .collect();
        assert_eq!(merged, expected, "{writes:?}");
    }

    #[test]
    fn continuous_writes_are_merged() {
        use At::*;
        check_merged(
            &[(0, Offset(0), 10), (0, Offset(10), 5)],
            &[(0, Offset(0), 15)],
        );
        check_merged(&[(0, End, 10), (0, End, 5)], &[(0, End, 15)]);
        check_merged(
            &[(0, Offset(0), 1), (0, Offset(1), 1), (0, Offset(2), 1)],
            &[(0, Offset(0), 3)],
        );
    }

    #[test]
    fn elsewhere_writes_are_not_merged() {
        use At::*;
        let max = MAX_MERGED as u64;
        let separate: &[&[(usize, At, usize)]] = &[
            &[(0, Offset(0), 10), (0, Offset(20), 5)],
            &[(0, Offset(0), 10), (1, Offset(10), 5)],
            &[(0, Offset(0), 10), (0, End, 5)],
            &[(0, Offset(10), 5), (0, Offset(0), 10)],
            &[(0, Offset(0), MAX_MERGED), (0, Offset(max), 1)],
        ];
        for &writes in separate {
            check_merged(writes, writes);
        }
    }

    #[track_caller]
    fn check_room(lens: &[usize], expected: usize) {
        let dir = TempDir::new().unwrap();
        let (file, id) = open(dir.path(), "game.bin");
        let (writer, disk) = writer();
        let paused = disk.pause();
        let mut at = 0;
        for &len in lens {
            let data = Bytes::from(vec![0; len]);
            writer
                .push(file.clone(), id, At::Offset(at), data, 0)
                .unwrap();
            // Leaves a gap, so no writes merge.
            at += len as u64 + 1;
        }

        let context = format!(
            "{} writes of {} bytes",
            lens.len(),
            lens.iter().sum::<usize>()
        );
        let writable = writer.writable().now_or_never().is_some();
        assert_eq!(writable, expected > 0, "{context}");
        let mut granted = 0;
        assert_eq!(writer.grant(&mut granted).unwrap(), expected, "{context}");
        writer.release(granted);
        drop(paused);
        futures::executor::block_on(writer.drain()).unwrap();
    }

    #[tokio::test]
    async fn a_full_queue_holds_the_plugin_back() {
        check_room(&[], MAX_GRANT);
        check_room(&[MAX_QUEUED - 10], 10);
        check_room(&[MAX_QUEUED], 0);
        check_room(&vec![1; MAX_QUEUED_WRITES - 1], MAX_GRANT);
        check_room(&vec![1; MAX_QUEUED_WRITES + 1], 0);
    }

    #[tokio::test]
    async fn writes_past_the_largest_offset_fail_alone() {
        let dir = TempDir::new().unwrap();
        let (file, id) = open(dir.path(), "game.bin");
        let (writer, _) = writer();
        let push = |at, data: &'static str| writer.push(file.clone(), id, at, data.into(), 0);

        for at in [i64::MAX as u64, u64::MAX - 1, u64::MAX] {
            let error = push(At::Offset(at), "ab").unwrap_err();
            assert_eq!(ErrorCode::from(error), ErrorCode::Invalid, "at {at}");
        }
        assert_eq!(reserved(&writer), 0);
        push(At::Offset(0), "ab").unwrap();
        writer.drain().await.unwrap();
        assert_eq!(fs::read(dir.path().join("game.bin")).unwrap(), b"ab");
    }

    struct Panics;

    impl DownloadReporter for Panics {
        fn progress(&self, _phase: DownloadPhase, _processed: u64, _total: Option<u64>) {}

        fn received(&self, _bytes: u64) {}

        fn written(&self, _bytes: u64) {
            panic!("reporter panicked");
        }
    }

    #[tokio::test]
    async fn a_panicking_writer_thread_fails_the_queue() {
        let dir = TempDir::new().unwrap();
        let (file, id) = open(dir.path(), "game.bin");
        let writer = Writer::new(Some(Arc::new(Panics)));
        writer
            .push(file.clone(), id, At::End, "ab".into(), 0)
            .unwrap();

        let drain = tokio::time::timeout(Duration::from_secs(5), writer.drain());
        assert!(drain.await.expect("drain shouldn't hang").is_err());
        assert!(!writer.is_running());
        assert!(writer.push(file, id, At::End, "cd".into(), 0).is_err());
    }

    #[tokio::test]
    async fn a_failed_write_fails_every_later_one() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("game.bin"), b"").unwrap();
        let read_only = File::open(dir.path().join("game.bin")).unwrap();
        let id = FileId::of(&read_only).unwrap();
        let code = ErrorCode::from(write_at(&read_only, At::Offset(0), b"a").unwrap_err());
        let read_only = Arc::new(read_only);
        let (writer, _) = writer();

        let push =
            |data: &'static str| writer.push(read_only.clone(), id, At::Offset(0), data.into(), 0);
        push("a").unwrap();
        let error = writer.drain().await.unwrap_err();
        assert_eq!(ErrorCode::from(error), code, "keeps the OS error code");
        assert_eq!(ErrorCode::from(push("b").unwrap_err()), code);
        assert!(writer.grant(&mut 0).is_err());
        assert!(writer.drain_file(&read_only).await.is_err());
    }

    #[tokio::test]
    async fn a_cancelled_call_waits_only_for_the_write_in_flight() {
        let dir = TempDir::new().unwrap();
        let (file, id) = open(dir.path(), "game.bin");
        let (writer, disk) = writer();
        let paused = disk.pause();
        for at in [0, 10, 20, 30] {
            writer
                .push(file.clone(), id, At::Offset(at), "abc".into(), 0)
                .unwrap();
        }
        disk.wait_until_holding();
        let closed = Arc::downgrade(&file);
        drop(file);

        writer.abandon();
        assert_eq!(reserved(&writer), 3, "only the write in flight is left");
        let mut drain = pin!(writer.drain());
        assert!(drain.as_mut().now_or_never().is_none());
        drop(paused);
        drain.await.unwrap();
        assert!(
            closed.upgrade().is_none(),
            "the writer still holds the file"
        );
        assert_eq!(fs::read(dir.path().join("game.bin")).unwrap(), b"abc");
    }

    #[test]
    fn a_file_is_the_same_however_it_was_opened() {
        let dir = TempDir::new().unwrap();
        let (_, a) = open(dir.path(), "a");
        let (_, a_again) = open(dir.path(), "a");
        let (_, b) = open(dir.path(), "b");

        assert_eq!(a, a_again);
        assert_ne!(a, b);
    }

    #[test]
    fn the_ids_of_closed_files_are_forgotten() {
        let dir = TempDir::new().unwrap();
        let (writer, _) = writer();
        for i in 0..100 {
            let (file, id) = open(dir.path(), &i.to_string());
            assert_eq!(writer.id_of(&file).unwrap(), id, "file {i}");
        }
        assert!(writer.0.ids.lock().unwrap().len() <= 1);
    }

    struct Wasi {
        ctx: WasiCtx,
        table: ResourceTable,
    }

    impl WasiView for Wasi {
        fn ctx(&mut self) -> WasiCtxView<'_> {
            WasiCtxView {
                ctx: &mut self.ctx,
                table: &mut self.table,
            }
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Calls {
        Inline,
        Spawned,
    }

    struct Harness {
        dir: TempDir,
        wasi: Wasi,
        writer: Writer,
        disk: Arc<Disk>,
    }

    impl Harness {
        /// Entries ending in `/` are dirs.
        fn new(entries: &[&str], calls: Calls) -> Self {
            let dir = TempDir::new().unwrap();
            for entry in entries {
                match entry.strip_suffix('/') {
                    Some(subdir) => fs::create_dir_all(dir.path().join(subdir)).unwrap(),
                    None => fs::write(dir.path().join(entry), entry).unwrap(),
                }
            }
            let mut ctx = WasiCtx::builder();
            ctx.allow_blocking_current_thread(calls == Calls::Inline)
                .preopened_dir(dir.path(), "/download", FsPerms::ReadWrite)
                .unwrap();
            let wasi = Wasi {
                ctx: ctx.build(),
                table: ResourceTable::new(),
            };
            let (writer, disk) = writer();
            Self {
                dir,
                wasi,
                writer,
                disk,
            }
        }

        fn probe(&self) -> Probe {
            Probe {
                writer: self.writer.clone(),
                disk: self.disk.clone(),
                dir: self.dir.path().to_owned(),
            }
        }

        fn view(&mut self) -> (FsView<'_>, Resource<Descriptor>) {
            let mut view = FsView::new(self.wasi.filesystem(), &self.writer);
            let (root, _) = preopens::Host::get_directories(&mut view.fs)
                .unwrap()
                .remove(0);
            (view, root)
        }
    }

    struct Probe {
        writer: Writer,
        disk: Arc<Disk>,
        dir: PathBuf,
    }

    impl Probe {
        fn read(&self, name: &str) -> Vec<u8> {
            fs::read(self.dir.join(name)).unwrap()
        }

        fn granted(&self) -> usize {
            self.writer.0.queue.borrow().granted
        }

        async fn fail_a_write(&self, name: &str) {
            let read_only = Arc::new(File::open(self.dir.join(name)).unwrap());
            let id = self.writer.id_of(&read_only).unwrap();
            let push = self
                .writer
                .push(read_only, id, At::Offset(0), "x".into(), 0);
            push.unwrap();
            assert!(self.writer.drain().await.is_err());
        }

        /// Runs `call` while a write to `held` is queued, and checks whether it
        /// waited for that write.
        async fn check_waits<T: Debug>(
            &self,
            held: &Arc<File>,
            waits: bool,
            name: &str,
            call: impl Future<Output = FsResult<T>>,
        ) -> FsResult<T> {
            let paused = self.disk.pause();
            let id = self.writer.id_of(held).unwrap();
            let push = self.writer.push(held.clone(), id, At::End, "!".into(), 0);
            push.unwrap();
            let mut call = pin!(call);
            let early = call.as_mut().now_or_never();
            drop(paused);
            self.writer.drain().await.unwrap();

            match early {
                Some(result) => {
                    assert!(!waits, "{name} didn't wait");
                    result
                }
                None => {
                    assert!(waits, "{name} waited");
                    call.await
                }
            }
        }
    }

    async fn open_at(
        view: &mut FsView<'_>,
        root: &Resource<Descriptor>,
        path: &str,
        oflags: OpenFlags,
    ) -> FsResult<Resource<Descriptor>> {
        let flags = match oflags.contains(OpenFlags::DIRECTORY) {
            true => DescriptorFlags::READ,
            false => DescriptorFlags::READ | DescriptorFlags::WRITE,
        };
        let path_flags = PathFlags::empty();
        HostDescriptor::open_at(view, borrow(root), path_flags, path.into(), oflags, flags).await
    }

    fn file_of(view: &FsView<'_>, fd: &Resource<Descriptor>) -> Arc<File> {
        match view.fs.table.get(fd).unwrap() {
            Descriptor::File(file) => file.file.clone(),
            Descriptor::Dir(_) => panic!("not a file"),
        }
    }

    fn write_stream(view: &mut FsView<'_>, file: &Resource<Descriptor>, chunks: &[&str]) {
        let stream = view.write_via_stream(borrow(file), 0).unwrap();
        let output = view.fs.table.get_mut(&stream).unwrap();
        for chunk in chunks {
            output.check_write().unwrap();
            let chunk = Bytes::copy_from_slice(chunk.as_bytes());
            output.write(chunk).unwrap();
        }
        output.flush().unwrap();
        view.fs.table.delete(stream).unwrap();
    }

    /// `check_write` and `write` as a plugin calls them.
    fn check_write(table: &mut ResourceTable, stream: &Resource<DynOutputStream>) -> usize {
        streams::HostOutputStream::check_write(table, borrow(stream)).unwrap() as usize
    }

    fn write(
        table: &mut ResourceTable,
        stream: &Resource<DynOutputStream>,
        len: usize,
    ) -> StreamResult<()> {
        streams::HostOutputStream::write(table, borrow(stream), vec![b'x'; len])
    }

    fn traps(result: StreamResult<()>) -> bool {
        matches!(result, Err(StreamError::Trap(_)))
    }

    #[tokio::test]
    async fn a_stream_writes_only_what_check_write_granted() {
        let mut h = Harness::new(&[], Calls::Spawned);
        let probe = h.probe();
        let (mut view, root) = h.view();
        let file = open_at(&mut view, &root, "game.bin", OpenFlags::CREATE).await;
        let stream = view.write_via_stream(borrow(&file.unwrap()), 0).unwrap();
        let table = &mut *view.fs.table;

        for _ in 0..100 {
            assert!(traps(write(table, &stream, MAX_GRANT)));
        }
        assert_eq!(reserved(&probe.writer), 0);

        assert_eq!(check_write(table, &stream), MAX_GRANT);
        assert!(traps(write(table, &stream, MAX_GRANT + 1)));
        write(table, &stream, MAX_GRANT).unwrap();
        assert!(traps(write(table, &stream, 1)), "a grant is for one write");

        check_write(table, &stream);
        let zeroes = streams::HostOutputStream::write_zeroes(table, borrow(&stream), 10);
        zeroes.unwrap();
        let zeroes = streams::HostOutputStream::write_zeroes;
        assert!(traps(zeroes(table, borrow(&stream), MAX_GRANT as u64 + 1)));
        // A trap drops the plugin's streams.
        table.delete(stream).unwrap();

        probe.writer.drain().await.unwrap();
        assert_eq!(probe.read("game.bin").len(), MAX_GRANT + 10);
        assert_eq!(reserved(&probe.writer), 0);
    }

    #[tokio::test]
    async fn streams_together_never_queue_more_than_the_limit() {
        let mut h = Harness::new(&[], Calls::Spawned);
        let probe = h.probe();
        let (mut view, root) = h.view();
        let file = open_at(&mut view, &root, "game.bin", OpenFlags::CREATE).await;
        let file = file.unwrap();
        let full = MAX_QUEUED / MAX_GRANT;
        let streams: Vec<_> = (0..full * 2)
            .map(|_| view.write_via_stream(borrow(&file), 0).unwrap())
            .collect();
        let table = &mut *view.fs.table;
        let paused = probe.disk.pause();

        let grants: Vec<_> = streams.iter().map(|s| check_write(table, s)).collect();
        assert!(grants[..full].iter().all(|&grant| grant == MAX_GRANT));
        assert!(grants[full..].iter().all(|&grant| grant == 0));

        for (stream, grant) in streams.iter().zip(grants) {
            write(table, stream, grant).unwrap();
            assert!(reserved(&probe.writer) <= MAX_QUEUED);
        }

        for _ in 0..100 {
            let stream = view.write_via_stream(borrow(&file), 0).unwrap();
            let grant = check_write(view.fs.table, &stream);
            write(view.fs.table, &stream, grant).unwrap();
            assert!(reserved(&probe.writer) <= MAX_QUEUED);
        }
        drop(paused);
        probe.writer.drain().await.unwrap();
    }

    #[tokio::test]
    async fn unused_grants_are_given_back() {
        let mut h = Harness::new(&[], Calls::Spawned);
        let probe = h.probe();
        let (mut view, root) = h.view();
        let file = open_at(&mut view, &root, "game.bin", OpenFlags::CREATE).await;
        let stream = view.write_via_stream(borrow(&file.unwrap()), 0).unwrap();
        let table = &mut *view.fs.table;

        check_write(table, &stream);
        check_write(table, &stream);
        assert_eq!(
            probe.granted(),
            MAX_GRANT,
            "checking again replaces the grant"
        );
        write(table, &stream, 10).unwrap();
        assert_eq!(probe.granted(), 0, "a write gives back what it didn't use");
        check_write(table, &stream);
        write(table, &stream, 0).unwrap();
        assert_eq!(probe.granted(), 0);
        check_write(table, &stream);
        table.delete(stream).unwrap();
        assert_eq!(probe.granted(), 0, "dropping a stream gives back its grant");
        probe.writer.drain().await.unwrap();
    }

    #[tokio::test]
    async fn idle_streams_hold_no_grant_after_blocking_calls() {
        let mut h = Harness::new(&[], Calls::Spawned);
        let probe = h.probe();
        let (mut view, root) = h.view();
        let file = open_at(&mut view, &root, "game.bin", OpenFlags::CREATE).await;
        let file = file.unwrap();
        let mut streams = Vec::new();
        let timeout = Duration::from_secs(5);
        for i in 0..MAX_QUEUED / MAX_GRANT * 2 {
            let stream = view.write_via_stream(borrow(&file), i as u64 * 20).unwrap();
            let table = &mut *view.fs.table;
            let output = table.get_mut(&stream).unwrap();
            let write = output.blocking_write_and_flush(Bytes::from_static(b"0123456789"));
            let write = tokio::time::timeout(timeout, write).await;
            write
                .expect("idle streams shouldn't block new ones")
                .unwrap();
            assert_eq!(probe.granted(), 0);

            assert!(check_write(table, &stream) >= 10);
            streams::HostOutputStream::write(table, borrow(&stream), b"abcdefghij".into()).unwrap();
            let flush = streams::HostOutputStream::blocking_flush(table, borrow(&stream));
            tokio::time::timeout(timeout, flush).await.unwrap().unwrap();
            assert_eq!(probe.granted(), 0);
            streams.push(stream);
        }
        probe.writer.drain().await.unwrap();
        let contents = probe.read("game.bin");
        assert_eq!(contents, b"0123456789abcdefghij".repeat(streams.len()));
    }

    #[tokio::test]
    async fn a_stream_holding_a_grant_is_ready() {
        let mut h = Harness::new(&[], Calls::Spawned);
        let (mut view, root) = h.view();
        let file = open_at(&mut view, &root, "game.bin", OpenFlags::CREATE).await;
        let file = file.unwrap();
        let full = MAX_QUEUED / MAX_GRANT;
        let streams: Vec<_> = (0..=full)
            .map(|_| view.write_via_stream(borrow(&file), 0).unwrap())
            .collect();
        let table = &mut *view.fs.table;
        for stream in &streams[..full] {
            check_write(table, stream);
        }
        let is_ready = |table: &mut ResourceTable, stream: &Resource<DynOutputStream>| {
            let output = table.get_mut(stream).unwrap();
            output.ready().now_or_never().is_some()
        };

        let mut streams = streams.into_iter();
        let first = streams.next().unwrap();
        let last = streams.last().unwrap();
        assert!(is_ready(table, &first));
        assert!(!is_ready(table, &last));
        table.delete(first).unwrap();
        assert!(is_ready(table, &last));
    }

    /// Checks that `call` waits for writes qeueud while `paused` and returns
    /// its result once those writes complete.
    async fn after_writes<T>(paused: Paused, call: impl Future<Output = T>) -> T {
        let mut call = pin!(call);
        let early = call.as_mut().now_or_never();
        assert!(
            early.is_none(),
            "the call didn't wait for the queued writes"
        );
        drop(paused);
        call.await
    }

    #[tokio::test]
    async fn a_read_sees_the_writes_queued_before_it() {
        let mut h = Harness::new(&[], Calls::Inline);
        let probe = h.probe();
        let (mut view, root) = h.view();
        let file = open_at(&mut view, &root, "game.bin", OpenFlags::CREATE).await;
        let file = file.unwrap();

        let paused = probe.disk.pause();
        write_stream(&mut view, &file, &["hello ", "world"]);
        let read = HostDescriptor::read(&mut view, borrow(&file), 100, 0);
        let read = after_writes(paused, read).await;
        assert_eq!(read.unwrap().0, b"hello world");
    }

    #[tokio::test]
    async fn a_renamed_file_has_everything_written_before() {
        let mut h = Harness::new(&[], Calls::Inline);
        let probe = h.probe();
        let (mut view, root) = h.view();
        let file = open_at(&mut view, &root, "state.tmp", OpenFlags::CREATE).await;
        let file = file.unwrap();

        let paused = probe.disk.pause();
        write_stream(&mut view, &file, &["resume ", "here"]);
        HostDescriptor::drop(&mut view, file).unwrap();
        let rename = HostDescriptor::rename_at(
            &mut view,
            borrow(&root),
            "state.tmp".into(),
            borrow(&root),
            "state".into(),
        );
        after_writes(paused, rename).await.unwrap();

        assert_eq!(probe.read("state"), b"resume here");
        assert_eq!(probe.disk.landed(), 11);
    }

    #[tokio::test]
    async fn calls_on_a_file_wait_for_its_writes_only() {
        let mut h = Harness::new(&["game.bin", "other.bin"], Calls::Inline);
        let probe = h.probe();
        let (mut view, root) = h.view();
        let game = open_at(&mut view, &root, "game.bin", OpenFlags::empty()).await;
        let game = game.unwrap();
        let game_again = open_at(&mut view, &root, "game.bin", OpenFlags::empty()).await;
        let other = open_at(&mut view, &root, "other.bin", OpenFlags::empty()).await;

        // The same file through another descriptor is the same file.
        let cases = [
            (file_of(&view, &game), true),
            (file_of(&view, &game_again.unwrap()), true),
            (file_of(&view, &other.unwrap()), false),
        ];
        for (held, waits) in cases {
            let fd = || borrow(&game);
            let read = HostDescriptor::read(&mut view, fd(), 10, 0);
            probe.check_waits(&held, waits, "read", read).await.unwrap();
            let stat = HostDescriptor::stat(&mut view, fd());
            probe.check_waits(&held, waits, "stat", stat).await.unwrap();
            let sync = HostDescriptor::sync(&mut view, fd());
            probe.check_waits(&held, waits, "sync", sync).await.unwrap();
            let sync_data = HostDescriptor::sync_data(&mut view, fd());
            let sync_data = probe.check_waits(&held, waits, "sync_data", sync_data);
            sync_data.await.unwrap();
            let set_size = HostDescriptor::set_size(&mut view, fd(), 8);
            let set_size = probe.check_waits(&held, waits, "set_size", set_size);
            set_size.await.unwrap();
            let unchanged = NewTimestamp::NoChange;
            let set_times = HostDescriptor::set_times(&mut view, fd(), unchanged, unchanged);
            let set_times = probe.check_waits(&held, waits, "set_times", set_times);
            set_times.await.unwrap();
            let hash = HostDescriptor::metadata_hash(&mut view, fd());
            let hash = probe.check_waits(&held, waits, "metadata_hash", hash);
            hash.await.unwrap();
        }
    }

    #[tokio::test]
    async fn calls_that_do_not_depend_on_contents_never_wait() {
        let mut h = Harness::new(&["game.bin", "empty/"], Calls::Inline);
        let probe = h.probe();
        let (mut view, root) = h.view();
        let game = open_at(&mut view, &root, "game.bin", OpenFlags::empty()).await;
        let game = game.unwrap();
        let held = file_of(&view, &game);
        let no_flags = PathFlags::empty();

        let get_type = HostDescriptor::get_type(&mut view, borrow(&game));
        let get_type = probe.check_waits(&held, false, "get_type", get_type);
        get_type.await.unwrap();
        let get_flags = HostDescriptor::get_flags(&mut view, borrow(&game));
        let get_flags = probe.check_waits(&held, false, "get_flags", get_flags);
        get_flags.await.unwrap();
        let advise = HostDescriptor::advise(&mut view, borrow(&game), 0, 0, Advice::Normal);
        let advise = probe.check_waits(&held, false, "advise", advise);
        advise.await.unwrap();
        let list = HostDescriptor::read_directory(&mut view, borrow(&root));
        let list = probe.check_waits(&held, false, "read_directory", list);
        list.await.unwrap();
        let reopen = open_at(&mut view, &root, "game.bin", OpenFlags::empty());
        let reopen = probe.check_waits(&held, false, "reopen", reopen);
        reopen.await.unwrap();
        let resume = open_at(&mut view, &root, "game.bin", OpenFlags::CREATE);
        let resume = probe.check_waits(&held, false, "reopen to resume", resume);
        resume.await.unwrap();
        let mkdir = HostDescriptor::create_directory_at(&mut view, borrow(&root), "new".into());
        let mkdir = probe.check_waits(&held, false, "create_directory_at", mkdir);
        mkdir.await.unwrap();
        let rmdir = HostDescriptor::remove_directory_at(&mut view, borrow(&root), "empty".into());
        let rmdir = probe.check_waits(&held, false, "remove_directory_at", rmdir);
        rmdir.await.unwrap();
        let (at, from, to) = (borrow(&root), "game.bin".into(), "hard.bin".into());
        let link = HostDescriptor::link_at(&mut view, borrow(&root), no_flags, from, at, to);
        let link = probe.check_waits(&held, false, "link_at", link);
        link.await.unwrap();
        let symlink =
            HostDescriptor::symlink_at(&mut view, borrow(&root), "game.bin".into(), "link".into());
        match probe.check_waits(&held, false, "symlink_at", symlink).await {
            // Windows only allows symlinks in Developer Mode or as admin.
            Err(error)
                if cfg!(windows) && error.downcast_ref() == Some(&ErrorCode::NotPermitted) => {}
            result => result.unwrap(),
        }
    }

    #[tokio::test]
    async fn opening_a_new_file_or_a_dir_never_waits() {
        let mut h = Harness::new(&["game.bin", "data/"], Calls::Inline);
        let probe = h.probe();
        let (mut view, root) = h.view();
        let game = open_at(&mut view, &root, "game.bin", OpenFlags::empty()).await;
        let held = file_of(&view, &game.unwrap());
        let no_flags = PathFlags::empty();

        let create = OpenFlags::CREATE | OpenFlags::TRUNCATE;
        let new_file = open_at(&mut view, &root, "new.bin", create);
        let new_file = probe.check_waits(&held, false, "open new", new_file);
        new_file.await.unwrap();
        let in_dir = open_at(&mut view, &root, "data/level.pak", create);
        let in_dir = probe.check_waits(&held, false, "open new in dir", in_dir);
        in_dir.await.unwrap();
        let data = open_at(&mut view, &root, "data", OpenFlags::DIRECTORY);
        let data = probe.check_waits(&held, false, "open dir", data);
        data.await.unwrap();
        let stat = HostDescriptor::stat_at(&mut view, borrow(&root), no_flags, "data".into());
        let stat = probe.check_waits(&held, false, "stat dir", stat);
        stat.await.unwrap();
        let stat = HostDescriptor::stat_at(&mut view, borrow(&root), no_flags, "nope".into());
        let missing = probe.check_waits(&held, false, "stat missing", stat).await;
        assert_eq!(
            missing.unwrap_err().downcast_ref(),
            Some(&ErrorCode::NoEntry)
        );
    }

    #[tokio::test]
    async fn paths_that_could_have_writes_wait_for_all_of_them() {
        let entries = ["game.bin", "other.bin", "old.tmp", "trash.bin"];
        let mut h = Harness::new(&entries, Calls::Inline);
        let probe = h.probe();
        let (mut view, root) = h.view();
        let game = open_at(&mut view, &root, "game.bin", OpenFlags::empty()).await;
        let held = file_of(&view, &game.unwrap());
        let no_flags = PathFlags::empty();
        let at = || borrow(&root);
        let unchanged = NewTimestamp::NoChange;

        let reopen = open_at(&mut view, &root, "game.bin", OpenFlags::TRUNCATE);
        let reopen = probe.check_waits(&held, true, "reopen", reopen);
        reopen.await.unwrap();
        let stat = HostDescriptor::stat_at(&mut view, at(), no_flags, "other.bin".into());
        let stat = probe.check_waits(&held, true, "stat_at", stat);
        stat.await.unwrap();
        let times = HostDescriptor::set_times_at(
            &mut view,
            at(),
            no_flags,
            "other.bin".into(),
            unchanged,
            unchanged,
        );
        let times = probe.check_waits(&held, true, "set_times_at", times);
        times.await.unwrap();
        let hash = HostDescriptor::metadata_hash_at(&mut view, at(), no_flags, "other.bin".into());
        let hash = probe.check_waits(&held, true, "metadata_hash_at", hash);
        hash.await.unwrap();
        let rename =
            HostDescriptor::rename_at(&mut view, at(), "old.tmp".into(), at(), "new".into());
        let rename = probe.check_waits(&held, true, "rename_at", rename);
        rename.await.unwrap();
        let unlink = HostDescriptor::unlink_file_at(&mut view, at(), "trash.bin".into());
        let unlink = probe.check_waits(&held, true, "unlink_file_at", unlink);
        unlink.await.unwrap();
    }

    #[tokio::test]
    async fn writing_to_a_read_only_descriptor_fails_alone() {
        let mut h = Harness::new(&["game.bin"], Calls::Inline);
        let probe = h.probe();
        let (mut view, root) = h.view();
        let (no_flags, read) = (PathFlags::empty(), DescriptorFlags::READ);
        let open = HostDescriptor::open_at;
        let open = open(
            &mut view,
            borrow(&root),
            no_flags,
            "game.bin".into(),
            OpenFlags::empty(),
            read,
        );
        let read_only = open.await.unwrap();

        let write = HostDescriptor::write(&mut view, borrow(&read_only), b"x".to_vec(), 0).await;
        assert_eq!(
            write.unwrap_err().downcast_ref(),
            Some(&ErrorCode::BadDescriptor)
        );
        let stream = view.write_via_stream(borrow(&read_only), 0);
        assert_eq!(
            stream.unwrap_err().downcast_ref(),
            Some(&ErrorCode::BadDescriptor)
        );

        let game = open_at(&mut view, &root, "game.bin", OpenFlags::empty()).await;
        let write = HostDescriptor::write(&mut view, borrow(&game.unwrap()), b"x".to_vec(), 0);
        assert_eq!(write.await.unwrap(), 1);
        probe.writer.drain().await.unwrap();
    }

    #[tokio::test]
    async fn opening_reading_and_writing_fail_once_a_write_failed() {
        let mut h = Harness::new(&["game.bin"], Calls::Inline);
        let probe = h.probe();
        let (mut view, root) = h.view();
        let game = open_at(&mut view, &root, "game.bin", OpenFlags::empty()).await;
        let game = game.unwrap();
        let stream = view.read_via_stream(borrow(&game), 0).unwrap();
        probe.fail_a_write("game.bin").await;

        let input = view.fs.table.get_mut(&stream).unwrap();
        assert!(input.read(4).is_err());
        assert!(input.blocking_read(4).await.is_err());
        let stat = HostDescriptor::stat(&mut view, borrow(&game)).await;
        assert!(stat.is_err());
        let new_file = open_at(&mut view, &root, "new.bin", OpenFlags::CREATE).await;
        assert!(new_file.is_err());
        let write = HostDescriptor::write(&mut view, borrow(&game), b"x".to_vec(), 0).await;
        assert!(write.is_err());
    }

    #[tokio::test]
    async fn a_read_stream_waits_for_writes_queued_after_it_was_opened() {
        let mut h = Harness::new(&["game.bin"], Calls::Inline);
        let probe = h.probe();
        let (mut view, root) = h.view();
        let reading = open_at(&mut view, &root, "game.bin", OpenFlags::empty()).await;
        let writing = open_at(&mut view, &root, "game.bin", OpenFlags::empty()).await;
        let stream = view.read_via_stream(borrow(&reading.unwrap()), 0).unwrap();
        let input = view.fs.table.get_mut(&stream).unwrap();
        assert_eq!(input.blocking_read(4).await.unwrap(), "game");

        let paused = probe.disk.pause();
        write_stream(&mut view, &writing.unwrap(), &["gameWXYZ"]);
        let input = view.fs.table.get_mut(&stream).unwrap();
        assert_eq!(input.read(4).unwrap(), "", "the write is still queued");
        assert!(input.ready().now_or_never().is_none());
        drop(paused);
        input.ready().await;
        assert_eq!(input.read(4).unwrap(), "WXYZ");
    }

    #[tokio::test]
    async fn a_read_stream_ends_only_at_the_end_of_the_file() {
        let mut h = Harness::new(&["game.bin"], Calls::Inline);
        let (mut view, root) = h.view();
        let game = open_at(&mut view, &root, "game.bin", OpenFlags::empty()).await;
        let stream = view.read_via_stream(borrow(&game.unwrap()), 0).unwrap();
        let input = view.fs.table.get_mut(&stream).unwrap();

        let reads = [
            (0, Some("")),
            (5, Some("game.")),
            (0, Some("")),
            (100, Some("bin")),
            (1, None),
        ];
        for (size, expected) in reads {
            match (input.read(size), expected) {
                (Ok(data), Some(expected)) => assert_eq!(data, expected, "read({size})"),
                (Err(StreamError::Closed), None) => {}
                (result, _) => panic!("read({size}) gave {result:?}"),
            }
        }
    }

    #[tokio::test]
    async fn a_read_stream_never_returns_overwritten_data() {
        let mut h = Harness::new(&["game.bin"], Calls::Inline);
        let (mut view, root) = h.view();
        let game = open_at(&mut view, &root, "game.bin", OpenFlags::empty()).await;
        let game = game.unwrap();
        let stream = view.read_via_stream(borrow(&game), 0).unwrap();
        let input = view.fs.table.get_mut(&stream).unwrap();
        input.ready().await;
        assert_eq!(input.read(2).unwrap(), "ga");

        let write = HostDescriptor::write(&mut view, borrow(&game), b"XXXXXX".to_vec(), 2);
        write.await.unwrap();
        let input = view.fs.table.get_mut(&stream).unwrap();
        assert_eq!(input.blocking_read(6).await.unwrap(), "XXXXXX");
    }

    #[test]
    fn a_dropped_read_stream_closes_its_file() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .max_blocking_threads(1)
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let mut h = Harness::new(&["game.bin"], Calls::Inline);
            let probe = h.probe();
            let (mut view, root) = h.view();
            let game = open_at(&mut view, &root, "game.bin", OpenFlags::empty()).await;
            let game = game.unwrap();
            let opened = Arc::downgrade(&file_of(&view, &game));
            // Holds up any read sent to a blocking thread, as a slow disk would.
            let (release, gate) = std::sync::mpsc::channel::<()>();
            let busy = tokio::task::spawn_blocking(move || gate.recv());

            let stream = view.read_via_stream(borrow(&game), 0).unwrap();
            let _ = view.fs.table.get_mut(&stream).unwrap().read(4);
            view.fs.table.delete(stream).unwrap();
            HostDescriptor::drop(&mut view, game).unwrap();
            let still_open = opened.upgrade().is_some();
            let removed = fs::remove_file(probe.dir.join("game.bin"));

            release.send(()).unwrap();
            busy.await.unwrap().unwrap();
            assert!(!still_open, "a read outlived the stream");
            removed.unwrap();
        });
    }

    /// A random file operation, applied to the download filesystem and to an
    /// in-memory model.
    #[derive(Debug)]
    enum Op {
        Open {
            name: usize,
            truncate: bool,
        },
        Close(usize),
        Stream {
            fd: usize,
            at: Option<u64>,
            chunks: Vec<usize>,
        },
        Write {
            fd: usize,
            at: u64,
            len: usize,
        },
        Read {
            fd: usize,
            at: u64,
            len: u64,
        },
        ReadStream {
            fd: usize,
            at: u64,
        },
        /// Reads on from the file's kept stream, or a new one from `at` within
        /// the file.
        KeepReading {
            fd: usize,
            at: u64,
            len: usize,
        },
        Stat(usize),
        StatAt(usize),
        SetSize {
            fd: usize,
            len: u64,
        },
        Sync(usize),
        Rename {
            from: usize,
            to: usize,
        },
        Unlink(usize),
    }

    const NAMES: [&str; 4] = ["a.bin", "b.bin", "data/c.bin", "data/d.bin"];

    fn random_op(rng: &mut StdRng, open: usize) -> Op {
        let size = |rng: &mut StdRng| match rng.random_range(0..10) {
            0 => rng.random_range(MAX_MERGED / 2..MAX_MERGED * 2),
            1..3 => rng.random_range(10_000..200_000),
            _ => rng.random_range(1..5_000),
        };
        let name = rng.random_range(0..NAMES.len());
        if open == 0 {
            return Op::Open {
                name,
                truncate: rng.random_bool(0.2),
            };
        }
        let fd = rng.random_range(0..open);
        let at = rng.random_range(0..300_000);
        match rng.random_range(0..16) {
            0 => Op::Open {
                name,
                truncate: rng.random_bool(0.2),
            },
            1 => Op::Close(fd),
            2..5 => {
                let chunks = (0..rng.random_range(1..4)).map(|_| size(rng)).collect();
                let at = rng.random_bool(0.8).then_some(at);
                Op::Stream { fd, at, chunks }
            }
            5 => Op::Write {
                fd,
                at,
                len: size(rng),
            },
            6 => Op::Read {
                fd,
                at,
                len: rng.random_range(0..300_000),
            },
            7 => Op::ReadStream { fd, at },
            8..10 => Op::KeepReading {
                fd,
                at,
                len: rng.random_range(1..20_000),
            },
            10 => Op::Stat(fd),
            11 => Op::StatAt(name),
            12 => Op::SetSize {
                fd,
                len: rng.random_range(0..400_000),
            },
            13 => Op::Sync(fd),
            14 => Op::Rename {
                from: name,
                to: rng.random_range(0..NAMES.len()),
            },
            _ => Op::Unlink(name),
        }
    }

    /// Bytes that differ by position and by write, so misplaced or stale data
    /// shows.
    fn pattern(seed: u64, len: usize) -> Vec<u8> {
        (0..len)
            .map(|i| (seed as usize * 31 + i * 7 % 251) as u8)
            .collect()
    }

    fn write_model(model: &mut Vec<u8>, at: usize, data: &[u8]) {
        if model.len() < at + data.len() {
            model.resize(at + data.len(), 0);
        }
        model[at..at + data.len()].copy_from_slice(data);
    }

    struct Pacer {
        disk: Arc<Disk>,
        paused: Option<Paused>,
        /// Calls that couldn't finish at once.
        waits: usize,
    }

    impl Pacer {
        fn new(disk: Arc<Disk>, slow: bool) -> Self {
            let paused = slow.then(|| disk.pause());
            Self {
                disk,
                paused,
                waits: 0,
            }
        }

        async fn run<T>(&mut self, call: impl Future<Output = T>) -> T {
            let mut call = pin!(call);
            if let Some(output) = call.as_mut().now_or_never() {
                return output;
            }
            self.waits += 1;
            let slow = self.paused.take().is_some();
            let output = call.await;
            if slow {
                self.paused = Some(self.disk.pause());
            }
            output
        }
    }

    async fn read_stream(
        view: &mut FsView<'_>,
        pacer: &mut Pacer,
        fd: &Resource<Descriptor>,
        at: u64,
    ) -> Vec<u8> {
        let stream = view.read_via_stream(borrow(fd), at).unwrap();
        let input = view.fs.table.get_mut(&stream).unwrap();
        let mut data = Vec::new();
        loop {
            match pacer.run(input.blocking_read(64 * 1024)).await {
                Ok(chunk) => data.extend_from_slice(&chunk),
                Err(StreamError::Closed) => break,
                Err(error) => panic!("reading failed: {error:?}"),
            }
        }
        view.fs.table.delete(stream).unwrap();
        data
    }

    /// Runs `count` random operations and returns how many had to wait.
    async fn check_random_ops(seed: u64, count: usize, calls: Calls, slow: bool) -> usize {
        let mut h = Harness::new(&["data/"], calls);
        let probe = h.probe();
        let (mut view, root) = h.view();
        let mut pacer = Pacer::new(probe.disk.clone(), slow);
        let mut rng = StdRng::seed_from_u64(seed);
        let mut model: BTreeMap<&str, Vec<u8>> = BTreeMap::new();
        let mut fds: Vec<(&str, Resource<Descriptor>)> = Vec::new();
        // A read stream per file, kept across operations as wasi-libc does.
        let mut kept: BTreeMap<&str, (Resource<DynInputStream>, usize)> = BTreeMap::new();
        let mut written = 0;
        let mut ops = Vec::new();

        for step in 0..count {
            let op = random_op(&mut rng, fds.len());
            let context = format!("seed {seed}, step {step}: {op:?}");
            match op {
                Op::Open { name, truncate } => {
                    let name = NAMES[name];
                    let mut oflags = OpenFlags::CREATE;
                    if truncate {
                        oflags |= OpenFlags::TRUNCATE;
                    }
                    let open = open_at(&mut view, &root, name, oflags);
                    let fd = pacer.run(open).await.expect(&context);
                    let contents = model.entry(name).or_default();
                    if truncate {
                        contents.clear();
                    }
                    fds.push((name, fd));
                }
                Op::Close(fd) => {
                    let (_, fd) = fds.remove(fd);
                    HostDescriptor::drop(&mut view, fd).unwrap();
                }
                Op::Stream { fd, at, chunks } => {
                    let (name, fd) = &fds[fd];
                    let stream = match at {
                        Some(at) => view.write_via_stream(borrow(fd), at),
                        None => view.append_via_stream(borrow(fd)),
                    };
                    let stream = stream.expect(&context);
                    let mut position = at.map_or(model[name].len(), |at| at as usize);
                    for len in chunks {
                        let data = pattern(seed ^ step as u64, len);
                        let output = view.fs.table.get_mut(&stream).unwrap();
                        let write = output.blocking_write_and_flush(Bytes::from(data.clone()));
                        pacer.run(write).await.expect(&context);
                        write_model(model.get_mut(name).unwrap(), position, &data);
                        position += len;
                        written += len as u64;
                    }
                    view.fs.table.delete(stream).unwrap();
                }
                Op::Write { fd, at, len } => {
                    let (name, fd) = &fds[fd];
                    let data = pattern(seed ^ step as u64, len);
                    let write = HostDescriptor::write(&mut view, borrow(fd), data.clone(), at);
                    let write = pacer.run(write).await.expect(&context);
                    assert_eq!(write, len as u64, "{context}");
                    write_model(model.get_mut(name).unwrap(), at as usize, &data);
                    written += len as u64;
                }
                Op::Read { fd, at, len } => {
                    let (name, fd) = &fds[fd];
                    let read = HostDescriptor::read(&mut view, borrow(fd), len, at);
                    let (data, _) = pacer.run(read).await.expect(&context);
                    let contents = &model[name];
                    let start = (at as usize).min(contents.len());
                    let end = (at + len).min(contents.len() as u64) as usize;
                    // Reads may be short, but never empty before the end.
                    let expected = &contents[start..end];
                    assert!(expected.starts_with(&data), "{context}: stale read");
                    assert!(
                        !data.is_empty() || expected.is_empty(),
                        "{context}: empty read"
                    );
                }
                Op::ReadStream { fd, at } => {
                    let (name, fd) = &fds[fd];
                    let data = read_stream(&mut view, &mut pacer, fd, at).await;
                    let contents = &model[name];
                    let start = (at as usize).min(contents.len());
                    assert!(data == contents[start..], "{context}: stale stream read");
                }
                Op::KeepReading { fd, at, len } => {
                    let (name, fd) = &fds[fd];
                    let (stream, position) = match kept.remove(name) {
                        Some(kept) => kept,
                        None => {
                            let at = at % (model[name].len() as u64 + 1);
                            let stream = view.read_via_stream(borrow(fd), at);
                            (stream.expect(&context), at as usize)
                        }
                    };
                    let input = view.fs.table.get_mut(&stream).unwrap();
                    let contents = &model[name];
                    match pacer.run(input.blocking_read(len)).await {
                        Ok(data) => {
                            let end = position + data.len();
                            assert!(end <= contents.len(), "{context}: read past the end");
                            let expected = &contents[position..end];
                            assert!(data == expected, "{context}: stale kept stream read");
                            kept.insert(name, (stream, end));
                        }
                        Err(StreamError::Closed) => {
                            assert!(position >= contents.len(), "{context}: early end");
                            view.fs.table.delete(stream).unwrap();
                        }
                        Err(error) => panic!("{context}: {error:?}"),
                    }
                }
                Op::Stat(fd) => {
                    let (name, fd) = &fds[fd];
                    let stat = HostDescriptor::stat(&mut view, borrow(fd));
                    let size = pacer.run(stat).await.expect(&context).size;
                    assert_eq!(size, model[name].len() as u64, "{context}");
                }
                Op::StatAt(name) => {
                    let name = NAMES[name];
                    let path_flags = PathFlags::empty();
                    let stat =
                        HostDescriptor::stat_at(&mut view, borrow(&root), path_flags, name.into());
                    let stat = pacer.run(stat).await;
                    match model.get(name) {
                        Some(contents) => {
                            let size = stat.expect(&context).size;
                            assert_eq!(size, contents.len() as u64, "{context}");
                        }
                        None => assert!(stat.is_err(), "{context}"),
                    }
                }
                Op::SetSize { fd, len } => {
                    let (name, fd) = &fds[fd];
                    let set_size = HostDescriptor::set_size(&mut view, borrow(fd), len);
                    pacer.run(set_size).await.expect(&context);
                    model.get_mut(name).unwrap().resize(len as usize, 0);
                }
                Op::Sync(fd) => {
                    let (_, fd) = &fds[fd];
                    let sync = HostDescriptor::sync(&mut view, borrow(fd));
                    pacer.run(sync).await.expect(&context);
                }
                Op::Rename { from, to } => {
                    let (from, to) = (NAMES[from], NAMES[to]);
                    if from == to || !model.contains_key(from) {
                        continue;
                    }
                    for (_, fd) in fds.extract_if(.., |(name, _)| *name == from || *name == to) {
                        HostDescriptor::drop(&mut view, fd).unwrap();
                    }
                    let renamed = kept.extract_if(.., |name, _| *name == from || *name == to);
                    for (_, (stream, _)) in renamed {
                        view.fs.table.delete(stream).unwrap();
                    }
                    let rename = HostDescriptor::rename_at(
                        &mut view,
                        borrow(&root),
                        from.into(),
                        borrow(&root),
                        to.into(),
                    );
                    pacer.run(rename).await.expect(&context);
                    let contents = model.remove(from).unwrap();
                    model.insert(to, contents);
                }
                Op::Unlink(name) => {
                    let name = NAMES[name];
                    if !model.contains_key(name) {
                        continue;
                    }
                    for (_, fd) in fds.extract_if(.., |(open, _)| *open == name) {
                        HostDescriptor::drop(&mut view, fd).unwrap();
                    }
                    if let Some((stream, _)) = kept.remove(name) {
                        view.fs.table.delete(stream).unwrap();
                    }
                    let unlink =
                        HostDescriptor::unlink_file_at(&mut view, borrow(&root), name.into());
                    pacer.run(unlink).await.expect(&context);
                    model.remove(name);
                }
            }
            ops.push(context);
        }

        let waits = pacer.waits;
        drop(pacer);
        probe.writer.drain().await.unwrap();
        for (_, fd) in fds {
            HostDescriptor::drop(&mut view, fd).unwrap();
        }
        for (stream, _) in kept.into_values() {
            view.fs.table.delete(stream).unwrap();
        }
        for name in NAMES {
            let on_disk = fs::read(probe.dir.join(name)).ok();
            assert!(
                on_disk.as_ref() == model.get(name),
                "seed {seed}: {name} differs after:\n{}",
                ops.join("\n")
            );
        }
        assert_eq!(probe.disk.landed(), written, "seed {seed}");
        waits
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn random_operations_match_a_model() {
        let mut inline_waits = 0;
        for seed in 0..40 {
            let calls = match seed % 4 {
                0 | 1 => Calls::Spawned,
                _ => Calls::Inline,
            };
            let slow = seed % 2 == 1;
            let waits = check_random_ops(seed, 60, calls, slow).await;
            if calls == Calls::Inline && slow {
                inline_waits += waits;
            }
        }
        assert!(inline_waits > 0, "no call ran while writes were queued");
    }
}
