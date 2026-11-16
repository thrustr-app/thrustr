use crate::plugin::{state::DOWNLOAD_MOUNT, windows};
use bytes::{Bytes, BytesMut};
use futures::future::BoxFuture;
use std::{
    collections::HashSet,
    fs::File,
    io,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll, ready},
};
use sys::{At, read_at, write_at};
use tokio::sync::oneshot;
use wasmtime::{
    AsContextMut, StoreContextMut,
    component::{
        Access, Accessor, Destination, FutureReader, HasData, Linker, Resource, ResourceTableError,
        Source, StreamConsumer, StreamProducer, StreamReader, StreamResult,
    },
};
use wasmtime_wasi::{
    OpenMode,
    filesystem::{Descriptor, WasiFilesystem, WasiFilesystemCtxView},
    p3::{
        bindings::filesystem::{
            preopens,
            types::{
                self, Advice, DescriptorFlags, DescriptorStat, DescriptorType, DirectoryEntry,
                ErrorCode, Filesize, HostDescriptor, HostDescriptorWithStore, MetadataHashValue,
                NewTimestamp, OpenFlags, PathFlags,
            },
        },
        filesystem::{FilesystemError as FsError, FilesystemResult as FsResult},
    },
};
use writer::FileId;

mod sys;
mod writer;

pub use writer::Writer;

/// Largest write a stream queues at once. Each is copied into a fresh buffer,
/// and keeping them this small lets the allocator reuse them, which is faster
/// on Windows.
const MAX_WRITE: usize = 256 * 1024;
/// Largest read a stream hands to a host reader at once.
const MAX_READ: usize = 64 * 1024;

type Stock = WasiFilesystem;

pub fn add_to_linker<T: FsState>(linker: &mut Linker<T>) -> wasmtime::Result<()> {
    types::add_to_linker::<T, Fs>(linker, T::fs)?;
    preopens::add_to_linker::<T, Fs>(linker, T::fs)?;
    Ok(())
}

pub trait FsState: Send + 'static {
    fn fs(&mut self) -> FsView<'_>;
}

pub struct Fs;

impl HasData for Fs {
    type Data<'a> = FsView<'a>;
}

pub struct FsView<'a> {
    fs: WasiFilesystemCtxView<'a>,
    writer: &'a Writer,
    queued: &'a mut QueuedFds,
    #[cfg_attr(not(windows), expect(dead_code))]
    allow_blocking_current_thread: bool,
}

impl<'a> FsView<'a> {
    pub fn new(
        fs: WasiFilesystemCtxView<'a>,
        writer: &'a Writer,
        queued: &'a mut QueuedFds,
        allow_blocking_current_thread: bool,
    ) -> Self {
        Self {
            fs,
            writer,
            queued,
            allow_blocking_current_thread,
        }
    }
}

#[derive(Default)]
pub struct QueuedFds(HashSet<u32>);

impl QueuedFds {
    fn contains(&self, fd: &Resource<Descriptor>) -> bool {
        self.0.contains(&fd.rep())
    }

    fn insert(&mut self, fd: &Resource<Descriptor>) {
        self.0.insert(fd.rep());
    }

    fn remove(&mut self, fd: &Resource<Descriptor>) {
        self.0.remove(&fd.rep());
    }
}

impl types::Host for FsView<'_> {
    fn convert_error_code(&mut self, error: FsError) -> wasmtime::Result<ErrorCode> {
        types::Host::convert_error_code(&mut self.fs, error)
    }
}

impl HostDescriptor for FsView<'_> {
    fn drop(&mut self, fd: Resource<Descriptor>) -> wasmtime::Result<()> {
        self.queued.remove(&fd);
        HostDescriptor::drop(&mut self.fs, fd)
    }
}

impl<U: FsState> HostDescriptorWithStore<U> for Fs {
    fn read_via_stream(
        mut store: Access<'_, U, Self>,
        fd: Resource<Descriptor>,
        offset: Filesize,
    ) -> wasmtime::Result<(StreamReader<u8>, FutureReader<Result<(), ErrorCode>>)> {
        let Some(mut producer) = store.get().file_producer(&fd, offset)? else {
            return Stock::read_via_stream(stock_access(&mut store), fd, offset);
        };
        let (result, receiver) = oneshot::channel();
        producer.result = Some(result);
        Ok((
            StreamReader::new(&mut store, producer)?,
            FutureReader::new(&mut store, receiver)?,
        ))
    }

    fn write_via_stream(
        store: Access<'_, U, Self>,
        fd: Resource<Descriptor>,
        data: StreamReader<u8>,
        offset: Filesize,
    ) -> wasmtime::Result<FutureReader<Result<(), ErrorCode>>> {
        pipe_to_file(store, fd, data, At::Offset(offset))
    }

    fn append_via_stream(
        store: Access<'_, U, Self>,
        fd: Resource<Descriptor>,
        data: StreamReader<u8>,
    ) -> wasmtime::Result<FutureReader<Result<(), ErrorCode>>> {
        pipe_to_file(store, fd, data, At::End)
    }

    async fn advise(
        store: &Accessor<U, Self>,
        fd: Resource<Descriptor>,
        offset: Filesize,
        len: Filesize,
        advice: Advice,
    ) -> FsResult<()> {
        Stock::advise(&stock(store), fd, offset, len, advice).await
    }

    async fn sync_data(store: &Accessor<U, Self>, fd: Resource<Descriptor>) -> FsResult<()> {
        drain_file(store, &fd).await?;
        Stock::sync_data(&stock(store), fd).await
    }

    async fn get_flags(
        store: &Accessor<U, Self>,
        fd: Resource<Descriptor>,
    ) -> FsResult<DescriptorFlags> {
        Stock::get_flags(&stock(store), fd).await
    }

    async fn get_type(
        store: &Accessor<U, Self>,
        fd: Resource<Descriptor>,
    ) -> FsResult<DescriptorType> {
        Stock::get_type(&stock(store), fd).await
    }

    async fn set_size(
        store: &Accessor<U, Self>,
        fd: Resource<Descriptor>,
        size: Filesize,
    ) -> FsResult<()> {
        drain_file(store, &fd).await?;
        Stock::set_size(&stock(store), fd, size).await
    }

    async fn set_times(
        store: &Accessor<U, Self>,
        fd: Resource<Descriptor>,
        atim: NewTimestamp,
        mtim: NewTimestamp,
    ) -> FsResult<()> {
        drain_file(store, &fd).await?;
        Stock::set_times(&stock(store), fd, atim, mtim).await
    }

    fn read_directory(
        mut store: Access<'_, U, Self>,
        fd: Resource<Descriptor>,
    ) -> wasmtime::Result<(
        StreamReader<DirectoryEntry>,
        FutureReader<Result<(), ErrorCode>>,
    )> {
        Stock::read_directory(stock_access(&mut store), fd)
    }

    async fn sync(store: &Accessor<U, Self>, fd: Resource<Descriptor>) -> FsResult<()> {
        drain_file(store, &fd).await?;
        Stock::sync(&stock(store), fd).await
    }

    async fn create_directory_at(
        store: &Accessor<U, Self>,
        fd: Resource<Descriptor>,
        path: String,
    ) -> FsResult<()> {
        check_name(&path)?;
        Stock::create_directory_at(&stock(store), fd, path).await
    }

    async fn stat(store: &Accessor<U, Self>, fd: Resource<Descriptor>) -> FsResult<DescriptorStat> {
        drain_file(store, &fd).await?;
        Stock::stat(&stock(store), fd).await
    }

    async fn stat_at(
        store: &Accessor<U, Self>,
        fd: Resource<Descriptor>,
        path_flags: PathFlags,
        path: String,
    ) -> FsResult<DescriptorStat> {
        drain_path(store, &fd, path_flags, &path).await?;
        Stock::stat_at(&stock(store), fd, path_flags, path).await
    }

    async fn set_times_at(
        store: &Accessor<U, Self>,
        fd: Resource<Descriptor>,
        path_flags: PathFlags,
        path: String,
        atim: NewTimestamp,
        mtim: NewTimestamp,
    ) -> FsResult<()> {
        drain_path(store, &fd, path_flags, &path).await?;
        Stock::set_times_at(&stock(store), fd, path_flags, path, atim, mtim).await
    }

    async fn link_at(
        store: &Accessor<U, Self>,
        fd: Resource<Descriptor>,
        old_path_flags: PathFlags,
        old_path: String,
        new_fd: Resource<Descriptor>,
        new_path: String,
    ) -> FsResult<()> {
        check_same_mount(store, &fd, &new_fd)?;
        check_name(&new_path)?;
        let stock = stock(store);
        Stock::link_at(&stock, fd, old_path_flags, old_path, new_fd, new_path).await
    }

    async fn open_at(
        store: &Accessor<U, Self>,
        fd: Resource<Descriptor>,
        path_flags: PathFlags,
        path: String,
        mut oflags: OpenFlags,
        mut flags: DescriptorFlags,
    ) -> FsResult<Resource<Descriptor>> {
        if oflags.contains(OpenFlags::CREATE) {
            check_name(&path)?;
        }

        let Some(writer) = queue_of(store, &fd) else {
            return open(store, fd, path_flags, path, oflags, flags).await;
        };

        writer.check()?;

        // Only truncating depends on previous writes, so it's done after opening,
        // once the opened file's writes have landed.
        let truncate = oflags.contains(OpenFlags::TRUNCATE)
            && !oflags.contains(OpenFlags::DIRECTORY)
            && writer.is_running();

        if truncate {
            oflags &= !OpenFlags::TRUNCATE;
            // Truncating opens for reading too unless asked to write.
            if !flags.contains(DescriptorFlags::WRITE) {
                flags |= DescriptorFlags::READ | DescriptorFlags::WRITE;
            }
        }

        let opened = match open(store, fd, path_flags, path, oflags, flags).await {
            // Like truncating at open on Windows.
            Err(error)
                if cfg!(windows)
                    && truncate
                    && matches!(error.downcast_ref(), Some(ErrorCode::IsDirectory)) =>
            {
                return Err(ErrorCode::Invalid.into());
            }
            opened => opened?,
        };

        store.with(|mut store| store.get().queued.insert(&opened));

        if truncate && let Err(error) = Self::set_size(store, borrow(&opened), 0).await {
            store
                .with(|mut store| HostDescriptor::drop(&mut store.get(), opened))
                .map_err(FsError::trap)?;
            return Err(error);
        }

        Ok(opened)
    }

    async fn readlink_at(
        store: &Accessor<U, Self>,
        fd: Resource<Descriptor>,
        path: String,
    ) -> FsResult<String> {
        Stock::readlink_at(&stock(store), fd, path).await
    }

    async fn remove_directory_at(
        store: &Accessor<U, Self>,
        fd: Resource<Descriptor>,
        path: String,
    ) -> FsResult<()> {
        Stock::remove_directory_at(&stock(store), fd, path).await
    }

    /// Windows refuses to rename files that queued writes hold open.
    async fn rename_at(
        store: &Accessor<U, Self>,
        fd: Resource<Descriptor>,
        old_path: String,
        new_fd: Resource<Descriptor>,
        new_path: String,
    ) -> FsResult<()> {
        check_same_mount(store, &fd, &new_fd)?;
        check_name(&new_path)?;
        if let Some(writer) = queue_of(store, &fd) {
            writer.drain().await?;
        }
        Stock::rename_at(&stock(store), fd, old_path, new_fd, new_path).await
    }

    async fn symlink_at(
        store: &Accessor<U, Self>,
        fd: Resource<Descriptor>,
        src_path: String,
        dest_path: String,
    ) -> FsResult<()> {
        check_name(&dest_path)?;
        Stock::symlink_at(&stock(store), fd, src_path, dest_path).await
    }

    /// Windows refuses to unlink files that queued writes hold open.
    async fn unlink_file_at(
        store: &Accessor<U, Self>,
        fd: Resource<Descriptor>,
        path: String,
    ) -> FsResult<()> {
        if let Some(writer) = queue_of(store, &fd) {
            writer.drain().await?;
        }
        Stock::unlink_file_at(&stock(store), fd, path).await
    }

    async fn is_same_object(
        store: &Accessor<U, Self>,
        a: Resource<Descriptor>,
        b: Resource<Descriptor>,
    ) -> wasmtime::Result<bool> {
        Stock::is_same_object(&stock(store), a, b).await
    }

    async fn metadata_hash(
        store: &Accessor<U, Self>,
        fd: Resource<Descriptor>,
    ) -> FsResult<MetadataHashValue> {
        drain_file(store, &fd).await?;
        Stock::metadata_hash(&stock(store), fd).await
    }

    async fn metadata_hash_at(
        store: &Accessor<U, Self>,
        fd: Resource<Descriptor>,
        path_flags: PathFlags,
        path: String,
    ) -> FsResult<MetadataHashValue> {
        drain_path(store, &fd, path_flags, &path).await?;
        Stock::metadata_hash_at(&stock(store), fd, path_flags, path).await
    }
}

impl preopens::Host for FsView<'_> {
    fn get_directories(&mut self) -> wasmtime::Result<Vec<(Resource<Descriptor>, String)>> {
        let dirs = preopens::Host::get_directories(&mut self.fs)?;
        for (fd, name) in &dirs {
            if name == DOWNLOAD_MOUNT {
                self.queued.insert(fd);
            }
        }
        Ok(dirs)
    }
}

fn stock<U: FsState>(store: &Accessor<U, Fs>) -> Accessor<U, Stock> {
    store.with_getter(|state| state.fs().fs)
}

fn stock_access<'a, U: FsState>(store: &'a mut Access<'_, U, Fs>) -> Access<'a, U, Stock> {
    Access::new(store.as_context_mut(), |state| state.fs().fs)
}

fn pipe_to_file<U: FsState>(
    mut store: Access<'_, U, Fs>,
    fd: Resource<Descriptor>,
    mut data: StreamReader<u8>,
    at: At,
) -> wasmtime::Result<FutureReader<Result<(), ErrorCode>>> {
    let (result, receiver) = oneshot::channel();
    match store.get().file_consumer(&fd, at) {
        Ok(mut consumer) => {
            consumer.result = Some(result);
            data.pipe(&mut store, consumer)?;
        }
        Err(error) => {
            let code = error.downcast()?;
            data.close(&mut store)?;
            let _ = result.send(Err(code));
        }
    }
    FutureReader::new(&mut store, receiver)
}

impl FsView<'_> {
    fn file_consumer(&self, fd: &Resource<Descriptor>, at: At) -> FsResult<FileConsumer> {
        let file = match self.fs.table.get(fd)? {
            Descriptor::File(file) if file.open_mode.contains(OpenMode::WRITE) => file.file.clone(),
            _ => return Err(ErrorCode::BadDescriptor.into()),
        };
        let queue = match self.queue_of(fd) {
            Some(writer) => {
                let id = writer.id_of(&file)?;
                Some((writer, id))
            }
            None => None,
        };
        Ok(FileConsumer {
            file,
            at,
            queue,
            result: None,
            waiting: None,
        })
    }

    /// `None` for a directory.
    fn file_producer(
        &self,
        fd: &Resource<Descriptor>,
        offset: u64,
    ) -> Result<Option<FileProducer>, ResourceTableError> {
        let Descriptor::File(file) = self.fs.table.get(fd)? else {
            return Ok(None);
        };
        Ok(Some(FileProducer {
            file: file.file.clone(),
            position: offset,
            writer: self.queue_of(fd),
            result: None,
            waiting: None,
        }))
    }
}

async fn drain_file<U: FsState>(
    store: &Accessor<U, Fs>,
    fd: &Resource<Descriptor>,
) -> FsResult<()> {
    let queued = store.with(|mut store| -> FsResult<_> {
        let view = store.get();
        let Some(writer) = view.queue_of(fd) else {
            return Ok(None);
        };
        match view.fs.table.get(fd)? {
            Descriptor::File(file) => Ok(Some((writer, file.file.clone()))),
            Descriptor::Dir(_) => {
                writer.check()?;
                Ok(None)
            }
        }
    })?;
    if let Some((writer, file)) = queued {
        writer.drain_file(&file).await?;
    }
    Ok(())
}

/// Waits for all queued writes unless `path` is a directory or doesn't exist.
async fn drain_path<U: FsState>(
    store: &Accessor<U, Fs>,
    fd: &Resource<Descriptor>,
    path_flags: PathFlags,
    path: &str,
) -> FsResult<()> {
    let Some(writer) = queue_of(store, fd) else {
        return Ok(());
    };
    if writer.is_running() && may_have_writes(store, fd, path_flags, path).await {
        return Ok(writer.drain().await?);
    }
    Ok(writer.check()?)
}

async fn may_have_writes<U: FsState>(
    store: &Accessor<U, Fs>,
    fd: &Resource<Descriptor>,
    path_flags: PathFlags,
    path: &str,
) -> bool {
    let stat = Stock::stat_at(&stock(store), borrow(fd), path_flags, path.into()).await;
    match stat {
        Ok(stat) => !matches!(stat.type_, DescriptorType::Directory),
        Err(error) => !matches!(error.downcast_ref(), Some(ErrorCode::NoEntry)),
    }
}

fn check_name(path: &str) -> FsResult<()> {
    let invalid = |part| !matches!(part, "" | "." | "..") && !windows::is_plain_name(part);
    if cfg!(windows) && path.split(['/', '\\']).any(invalid) {
        return Err(ErrorCode::Invalid.into());
    }
    Ok(())
}

fn check_same_mount<U: FsState>(
    store: &Accessor<U, Fs>,
    fd: &Resource<Descriptor>,
    new_fd: &Resource<Descriptor>,
) -> FsResult<()> {
    let same = store.with(|mut store| {
        let queued = store.get().queued;
        queued.contains(fd) == queued.contains(new_fd)
    });
    if !same {
        return Err(ErrorCode::CrossDevice.into());
    }
    Ok(())
}

fn queue_of<U: FsState>(store: &Accessor<U, Fs>, fd: &Resource<Descriptor>) -> Option<Writer> {
    store.with(|mut store| store.get().queue_of(fd))
}

impl FsView<'_> {
    fn queue_of(&self, fd: &Resource<Descriptor>) -> Option<Writer> {
        self.queued.contains(fd).then(|| self.writer.clone())
    }
}

async fn open<U: FsState>(
    store: &Accessor<U, Fs>,
    fd: Resource<Descriptor>,
    path_flags: PathFlags,
    path: String,
    oflags: OpenFlags,
    flags: DescriptorFlags,
) -> FsResult<Resource<Descriptor>> {
    #[cfg(windows)]
    if let Some(opened) = store.with(|mut store| {
        store
            .get()
            .open_file_directly(&fd, path_flags, &path, oflags, flags)
    })? {
        return Ok(opened);
    }
    Stock::open_at(&stock(store), fd, path_flags, path, oflags, flags).await
}

impl FsView<'_> {
    /// Opens a file with one system call. `None` leaves the open to Wasmtime, which
    /// opens each directory on the way.
    #[cfg(windows)]
    fn open_file_directly(
        &mut self,
        fd: &Resource<Descriptor>,
        path_flags: PathFlags,
        path: &str,
        oflags: OpenFlags,
        flags: DescriptorFlags,
    ) -> FsResult<Option<Resource<Descriptor>>> {
        use windows::Disposition;

        let Descriptor::Dir(dir) = self.fs.table.get(fd)? else {
            return Ok(None);
        };
        let sync = DescriptorFlags::FILE_INTEGRITY_SYNC
            | DescriptorFlags::DATA_INTEGRITY_SYNC
            | DescriptorFlags::REQUESTED_WRITE_SYNC;
        if oflags.contains(OpenFlags::DIRECTORY) || flags.intersects(sync) {
            return Ok(None);
        }

        // As in Wasmtime's `Dir::open_at`.
        let create = oflags.contains(OpenFlags::CREATE);
        let exclusive = create && oflags.contains(OpenFlags::EXCLUSIVE);
        let truncate = oflags.contains(OpenFlags::TRUNCATE);
        let write = create || truncate || flags.contains(DescriptorFlags::WRITE);
        let read = flags.contains(DescriptorFlags::READ) || !flags.contains(DescriptorFlags::WRITE);
        if write && dir.perms.write_not_permitted() {
            return Ok(None);
        }

        // Wasmtime truncates after opening unless following symlinks.
        let truncate_after =
            truncate && !exclusive && !path_flags.contains(PathFlags::SYMLINK_FOLLOW);
        let disposition = match (exclusive, create, truncate && !truncate_after) {
            (true, ..) => Disposition::CreateNew,
            (false, false, false) => Disposition::Open,
            (false, true, false) => Disposition::OpenOrCreate,
            (false, false, true) => Disposition::Truncate,
            (false, true, true) => Disposition::TruncateOrCreate,
        };

        let Ok(file) = windows::open_file_beneath(&dir.dir, path, read, write, disposition) else {
            return Ok(None);
        };
        if truncate_after && file.set_len(0).is_err() {
            return Ok(None);
        }
        let mut open_mode = OpenMode::empty();
        open_mode.set(OpenMode::READ, read);
        open_mode.set(OpenMode::WRITE, write);
        let file = wasmtime_wasi::filesystem::File::new(
            file,
            dir.perms,
            open_mode,
            self.allow_blocking_current_thread,
        );
        Ok(Some(self.fs.table.push(Descriptor::File(file))?))
    }
}

fn borrow<T: 'static>(resource: &Resource<T>) -> Resource<T> {
    Resource::new_borrow(resource.rep())
}

/// Writes a stream to a file. Through the queue, writes count as done once
/// queued, like OS write-back. Otherwise they're performed on the calling task, so
/// none outlives the call.
struct FileConsumer {
    file: Arc<File>,
    at: At,
    queue: Option<(Writer, FileId)>,
    result: Option<oneshot::Sender<Result<(), ErrorCode>>>,
    waiting: Option<BoxFuture<'static, io::Result<()>>>,
}

impl FileConsumer {
    /// Writes as much of `data` as the queue has room for, waiting until it has any,
    /// or writes all of it when not queued. Returns the number of bytes written.
    fn poll_write(&mut self, cx: &mut Context<'_>, data: &[u8]) -> Poll<io::Result<usize>> {
        if let Some((writer, _)) = &self.queue
            && self.waiting.is_none()
            && writer.room() == 0
        {
            let writer = writer.clone();
            self.waiting = Some(Box::pin(async move { writer.writable().await }));
        }
        ready!(poll_waiting(&mut self.waiting, cx));
        let len = self.write(data)?;
        self.at = self.at.after(len);
        Poll::Ready(Ok(len))
    }

    fn write(&self, data: &[u8]) -> io::Result<usize> {
        match &self.queue {
            Some((writer, id)) => {
                let len = data.len().min(MAX_WRITE).min(writer.room());
                let data = Bytes::copy_from_slice(&data[..len]);
                writer.push(self.file.clone(), *id, self.at, data)?;
                Ok(len)
            }
            None => {
                self.at.check_fits(data.len())?;
                write_at(&self.file, self.at, data)?;
                Ok(data.len())
            }
        }
    }
}

impl<D> StreamConsumer<D> for FileConsumer {
    type Item = u8;

    fn poll_consume(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        store: StoreContextMut<D>,
        source: Source<'_, u8>,
        finish: bool,
    ) -> Poll<wasmtime::Result<StreamResult>> {
        let this = &mut *self;
        let mut source = source.as_direct(store);
        match this.poll_write(cx, source.remaining()) {
            Poll::Ready(Ok(len)) => {
                source.mark_read(len);
                Poll::Ready(Ok(StreamResult::Completed))
            }
            Poll::Ready(Err(error)) => {
                send_result(&mut this.result, Err(error));
                Poll::Ready(Ok(StreamResult::Dropped))
            }
            // Pending only while waiting for room, before anything is written, so a
            // cancel just stops waiting.
            Poll::Pending if finish => {
                this.waiting = None;
                Poll::Ready(Ok(StreamResult::Cancelled))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl Drop for FileConsumer {
    fn drop(&mut self) {
        let done = self
            .queue
            .as_ref()
            .map_or(Ok(()), |(writer, _)| writer.check());
        send_result(&mut self.result, done);
    }
}

/// Reads a file on the calling task, so no read outlives the call. Reads go
/// straight into the reader's buffer, so they never return data a write has
/// since replaced. With a writer, each read first waits for the file's queued
/// writes, even those queued after the stream was opened.
struct FileProducer {
    file: Arc<File>,
    position: u64,
    writer: Option<Writer>,
    result: Option<oneshot::Sender<Result<(), ErrorCode>>>,
    waiting: Option<BoxFuture<'static, io::Result<()>>>,
}

impl FileProducer {
    /// Reads into `buf` once the file's queued writes have landed. Returns
    /// how many bytes it read, 0 at the end. An empty `buf` asks whether the
    /// stream is ready, so it only waits.
    fn poll_read(&mut self, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<io::Result<usize>> {
        if let Some(writer) = &self.writer
            && self.waiting.is_none()
            && writer.has_pending(&self.file)
        {
            let (writer, file) = (writer.clone(), self.file.clone());
            self.waiting = Some(Box::pin(async move { writer.drain_file(&file).await }));
        }
        ready!(poll_waiting(&mut self.waiting, cx));
        if let Some(writer) = &self.writer {
            writer.check()?;
        }
        if buf.is_empty() {
            return Poll::Ready(Ok(0));
        }
        let len = read_at(&self.file, buf, self.position)?;
        self.position += len as u64;
        Poll::Ready(Ok(len))
    }
}

impl<D> StreamProducer<D> for FileProducer {
    type Item = u8;
    type Buffer = BytesMut;

    fn poll_produce<'a>(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        store: StoreContextMut<'a, D>,
        destination: Destination<'a, u8, BytesMut>,
        finish: bool,
    ) -> Poll<wasmtime::Result<StreamResult>> {
        let this = &mut *self;
        let mut destination = destination.as_direct(store, MAX_READ);
        let buf = destination.remaining();
        let empty = buf.is_empty();
        match this.poll_read(cx, buf) {
            Poll::Ready(Ok(0)) if !empty => {
                send_result(&mut this.result, Ok(()));
                Poll::Ready(Ok(StreamResult::Dropped))
            }
            Poll::Ready(Ok(len)) => {
                destination.mark_written(len);
                Poll::Ready(Ok(StreamResult::Completed))
            }
            Poll::Ready(Err(error)) => {
                send_result(&mut this.result, Err(error));
                Poll::Ready(Ok(StreamResult::Dropped))
            }
            // Pending only while waiting for queued writes, before anything is read, so
            // a cancel just stops waiting.
            Poll::Pending if finish => {
                this.waiting = None;
                Poll::Ready(Ok(StreamResult::Cancelled))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl Drop for FileProducer {
    fn drop(&mut self) {
        send_result(&mut self.result, Ok(()));
    }
}

/// A failed write shows in the next operation.
fn poll_waiting(
    waiting: &mut Option<BoxFuture<'static, io::Result<()>>>,
    cx: &mut Context<'_>,
) -> Poll<()> {
    if let Some(future) = waiting {
        let _ = ready!(future.as_mut().poll(cx));
        *waiting = None;
    }
    Poll::Ready(())
}

fn send_result(result: &mut Option<oneshot::Sender<Result<(), ErrorCode>>>, done: io::Result<()>) {
    if let Some(result) = result.take() {
        let _ = result.send(done.map_err(ErrorCode::from));
    }
}

#[cfg(test)]
mod tests;
