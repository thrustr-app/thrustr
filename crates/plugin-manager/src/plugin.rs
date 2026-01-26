use crate::Storefront;
use anyhow::Result;
use domain::PluginManifest;
use wasmtime::Store;
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxView, WasiView};
use xutex::AsyncMutex;

pub struct Plugin {
    manifest: PluginManifest,
    storefront: Storefront,
    store: AsyncMutex<Store<PluginState>>,
}

impl Plugin {
    pub(crate) fn new(
        manifest: PluginManifest,
        storefront: Storefront,
        store: Store<PluginState>,
    ) -> Self {
        Self {
            manifest,
            storefront,
            store: AsyncMutex::new(store),
        }
    }

    pub fn id(&self) -> &str {
        &self.manifest.plugin.id
    }

    pub async fn init(&self) -> Result<(), String> {
        let mut store = self.store.lock().await;

        store
            .run_concurrent(async |accessor| {
                self.storefront
                    .thrustr_storefront_storefront_provider()
                    .call_init(accessor)
                    .await
            })
            .await
            .unwrap()
            .unwrap()
    }
}

pub struct PluginState {
    ctx: WasiCtx,
    table: ResourceTable,
}

impl PluginState {
    pub fn new() -> Self {
        let ctx = WasiCtx::builder().build();
        Self {
            ctx,
            table: ResourceTable::new(),
        }
    }
}

impl WasiView for PluginState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.ctx,
            table: &mut self.table,
        }
    }
}
