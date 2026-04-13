use crate::wit::thrustr::plugin::types::Host;
use domain::component::ComponentStorage;
use std::sync::Arc;
use wasmtime::component::HasData;
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxView, WasiView};
use wasmtime_wasi_http::{
    WasiHttpCtx,
    p2::{WasiHttpCtxView, WasiHttpView},
};

pub struct PluginState {
    ctx: WasiCtx,
    http_ctx: WasiHttpCtx,
    table: ResourceTable,
    pub(crate) id: String,
    pub(crate) storage: Arc<dyn ComponentStorage>,
}

impl PluginState {
    pub fn new(id: &str, storage: Arc<dyn ComponentStorage>) -> Self {
        let ctx = WasiCtx::builder()
            .inherit_network()
            .inherit_stdout()
            .build();

        Self {
            ctx,
            http_ctx: WasiHttpCtx::new(),
            table: ResourceTable::new(),
            id: id.to_owned(),
            storage,
        }
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
            hooks: Default::default(),
        }
    }
}
