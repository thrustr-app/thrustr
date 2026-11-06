use crate::{
    plugin::{
        PluginRuntime,
        host::{FsView, OutboundHttp, PluginCache, Writer},
    },
    wit::thrustr::plugin::types::Host,
};
use component::ComponentLink;
use domain::{
    component::{ComponentStorage, ConfigSchema},
    download::DownloadReporter,
};
use std::{path::PathBuf, sync::Arc};
use wasmtime::component::HasData;
use wasmtime::{StoreLimits, StoreLimitsBuilder};
use wasmtime_wasi::{
    FsPerms, ResourceTable, WasiCtx, WasiCtxView, WasiView, filesystem::WasiFilesystemCtxView,
};
use wasmtime_wasi_http::{WasiHttpCtx, WasiHttpCtxView, WasiHttpView};

const MAX_MEMORY: usize = 256 * 1024 * 1024;

pub struct PluginState {
    ctx: WasiCtx,
    http_ctx: WasiHttpCtx,
    hooks: OutboundHttp,
    table: ResourceTable,
    limits: StoreLimits,
    pub(crate) id: String,
    pub(crate) storage: Arc<dyn ComponentStorage>,
    pub(crate) link: ComponentLink,
    pub(crate) cache: Arc<PluginCache>,
    pub(crate) config_schema: Option<Arc<ConfigSchema>>,
    pub(crate) reporter: Option<Arc<dyn DownloadReporter>>,
    writer: Writer,
}

pub(crate) struct DownloadCall {
    pub(crate) dir: PathBuf,
    pub(crate) reporter: Arc<dyn DownloadReporter>,
}

impl PluginState {
    pub(crate) fn new(
        runtime: &PluginRuntime,
        download: Option<DownloadCall>,
        writer: Writer,
    ) -> wasmtime::Result<Self> {
        let mut ctx = WasiCtx::builder();
        // Stock Wasmtime fs calls run inline, so none outlives a cancelled call.
        ctx.inherit_stdout().allow_blocking_current_thread(true);
        let reporter = match download {
            Some(download) => {
                ctx.preopened_dir(&download.dir, "/download", FsPerms::ReadWrite)?;
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
            id: runtime.id.clone(),
            storage: runtime.storage.clone(),
            link: runtime.link.clone(),
            cache: runtime.cache.clone(),
            config_schema: runtime.config_schema.clone(),
            reporter,
            writer,
        })
    }

    pub(crate) fn limits(&mut self) -> &mut StoreLimits {
        &mut self.limits
    }

    pub(crate) fn fs(&mut self) -> FsView<'_> {
        let fs = WasiFilesystemCtxView {
            ctx: self.ctx.filesystem(),
            table: &mut self.table,
        };
        FsView::new(fs, &self.writer)
    }
}

impl Host for PluginState {}

impl HasData for PluginState {
    type Data<'a> = &'a mut PluginState;
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
