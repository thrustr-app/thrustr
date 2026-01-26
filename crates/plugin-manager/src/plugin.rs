use crate::{Storefront, thrustr::storefront::kv_store};
use anyhow::Result;
use domain::{PluginManifest, Storage};
use std::sync::Arc;
use wasmtime::{Store, component::HasData};
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

        self.storefront
            .thrustr_storefront_storefront_provider()
            .call_init(&mut *store)
            .await
            .unwrap()
    }
}

pub struct PluginState {
    ctx: WasiCtx,
    table: ResourceTable,
    id: String,
    storage: Arc<dyn Storage>,
}

impl PluginState {
    pub fn new(id: &str, storage: Arc<dyn Storage>) -> Self {
        let ctx = WasiCtx::builder().inherit_stdout().build();
        Self {
            ctx,
            table: ResourceTable::new(),
            id: id.to_owned(),
            storage,
        }
    }
}

impl kv_store::Host for PluginState {
    async fn get(&mut self, key: String) -> Result<Option<Vec<u8>>, kv_store::Error> {
        self.storage
            .get_plugin_data(&self.id, &key)
            .map_err(|e| kv_store::Error::Internal(e.to_string()))
    }

    async fn set(&mut self, key: String, value: Vec<u8>) -> Result<(), kv_store::Error> {
        self.storage
            .set_plugin_data(&self.id, &key, value)
            .map_err(|e| kv_store::Error::Internal(e.to_string()))
    }
}

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
