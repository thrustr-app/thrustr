use crate::{
    plugin::{
        PluginRuntime,
        host::{FsState, FsView, OutboundHttp, QueuedFds, Writer},
    },
    wit::thrustr::plugin::types::Host,
};
use domain::download::DownloadReporter;
use std::{fs, path::PathBuf, sync::Arc};
use wasmtime::{StoreLimits, StoreLimitsBuilder, component::HasData, error::Context};
use wasmtime_wasi::{
    FsPerms, ResourceTable, WasiCtx, WasiCtxView, WasiView, filesystem::WasiFilesystemCtxView,
};
use wasmtime_wasi_http::{WasiHttpCtx, WasiHttpCtxView, WasiHttpView};

const MAX_MEMORY: usize = 256 * 1024 * 1024;
/// Stock Wasmtime fs calls run inline, so none outlives a cancelled call.
const ALLOW_BLOCKING_CURRENT_THREAD: bool = true;

pub const DOWNLOAD_MOUNT: &str = "/download";
pub const DATA_MOUNT: &str = "/data";

pub struct PluginState {
    pub runtime: Arc<PluginRuntime>,
    pub reporter: Option<Arc<dyn DownloadReporter>>,
    pub limits: StoreLimits,
    ctx: WasiCtx,
    http_ctx: WasiHttpCtx,
    hooks: OutboundHttp,
    table: ResourceTable,
    writer: Writer,
    queued_fds: QueuedFds,
}

pub struct DownloadCall {
    pub dir: PathBuf,
    pub reporter: Arc<dyn DownloadReporter>,
}

impl PluginState {
    pub fn new(
        runtime: Arc<PluginRuntime>,
        download: Option<DownloadCall>,
        writer: Writer,
    ) -> wasmtime::Result<Self> {
        let mut ctx = WasiCtx::builder();
        ctx.inherit_stdout()
            .allow_blocking_current_thread(ALLOW_BLOCKING_CURRENT_THREAD);

        let data_dir = &runtime.data_dir;
        if ctx
            .preopened_dir(data_dir, DATA_MOUNT, FsPerms::ReadWrite)
            .is_err()
        {
            fs::create_dir_all(data_dir).with_context(|| {
                format!("failed to create data directory {}", data_dir.display())
            })?;
            ctx.preopened_dir(data_dir, DATA_MOUNT, FsPerms::ReadWrite)
                .with_context(|| format!("failed to open data directory {}", data_dir.display()))?;
        }

        let reporter = match download {
            Some(download) => {
                ctx.preopened_dir(&download.dir, DOWNLOAD_MOUNT, FsPerms::ReadWrite)?;
                Some(download.reporter)
            }
            None => None,
        };

        Ok(Self {
            ctx: ctx.build(),
            http_ctx: WasiHttpCtx::new(),
            hooks: OutboundHttp::new(
                runtime.http_client.clone(),
                runtime.allowed_hosts.clone(),
                reporter.clone(),
            ),
            table: ResourceTable::new(),
            limits: StoreLimitsBuilder::new().memory_size(MAX_MEMORY).build(),
            runtime,
            reporter,
            writer,
            queued_fds: QueuedFds::default(),
        })
    }
}

impl FsState for PluginState {
    fn fs(&mut self) -> FsView<'_> {
        let fs = WasiFilesystemCtxView {
            ctx: self.ctx.filesystem(),
            table: &mut self.table,
        };
        FsView::new(
            fs,
            &self.writer,
            &mut self.queued_fds,
            ALLOW_BLOCKING_CURRENT_THREAD,
        )
    }
}

impl Host for PluginState {}

impl HasData for PluginState {
    type Data<'a> = &'a mut Self;
}

impl WasiView for PluginState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.ctx,
            table: &mut self.table,
        }
    }
}

impl WasiHttpView for PluginState {
    fn http(&mut self) -> WasiHttpCtxView<'_> {
        WasiHttpCtxView {
            ctx: &mut self.http_ctx,
            table: &mut self.table,
            hooks: &mut self.hooks,
        }
    }
}
