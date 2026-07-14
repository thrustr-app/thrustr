use crate::{plugin::host::OutboundHttp, wit::thrustr::plugin::types::Host};
use domain::component::ComponentStorage;
use reqwest::Client;
use std::sync::Arc;
use wasmtime::component::HasData;
use wasmtime::{StoreLimits, StoreLimitsBuilder};
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxView, WasiView};
use wasmtime_wasi_http::{
    WasiHttpCtx,
    p3::{WasiHttpCtxView, WasiHttpView},
};

const MAX_MEMORY: usize = 256 * 1024 * 1024;

pub struct PluginState {
    ctx: WasiCtx,
    http_ctx: WasiHttpCtx,
    hooks: OutboundHttp,
    table: ResourceTable,
    limits: StoreLimits,
    pub(crate) id: String,
    pub(crate) storage: Arc<dyn ComponentStorage>,
}

impl PluginState {
    pub fn new(
        id: &str,
        storage: Arc<dyn ComponentStorage>,
        http_client: Client,
        allowed_hosts: Arc<[String]>,
    ) -> Self {
        let ctx = WasiCtx::builder().inherit_stdout().build();

        Self {
            ctx,
            http_ctx: WasiHttpCtx::new(),
            hooks: OutboundHttp::new(http_client, allowed_hosts),
            table: ResourceTable::new(),
            limits: StoreLimitsBuilder::new().memory_size(MAX_MEMORY).build(),
            id: id.to_owned(),
            storage,
        }
    }

    pub(crate) fn limits(&mut self) -> &mut StoreLimits {
        &mut self.limits
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
