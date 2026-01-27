use crate::{
    Storefront,
    exports::thrustr::storefront::storefront_provider::Error as StorefrontProviderError,
    thrustr::storefront::kv_store::{Error as KvStoreError, Host as KvStoreHost},
};
use anyhow::Result;
use domain::{PluginManifest, Storage};
use std::sync::Arc;
use wasmtime::{Store, component::HasData};
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxView, WasiView};
use xutex::AsyncMutex;

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

impl KvStoreHost for PluginState {
    async fn get(&mut self, key: String) -> Result<Option<Vec<u8>>, KvStoreError> {
        self.storage
            .get_plugin_data(&self.id, &key)
            .map_err(|e| KvStoreError::Internal(e.to_string()))
    }

    async fn set(&mut self, key: String, value: Vec<u8>) -> Result<(), KvStoreError> {
        self.storage
            .set_plugin_data(&self.id, &key, value)
            .map_err(|e| KvStoreError::Internal(e.to_string()))
    }

    async fn delete(&mut self, key: String) -> Result<(), KvStoreError> {
        self.storage
            .delete_plugin_data(&self.id, &key)
            .map_err(|e| KvStoreError::Internal(e.to_string()))
    }

    async fn list(&mut self, prefix: Option<String>) -> Result<Vec<String>, KvStoreError> {
        self.storage
            .list_plugin_data(&self.id, prefix.as_deref())
            .map_err(|e| KvStoreError::Internal(e.to_string()))
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

    pub async fn init(&self) -> Result<(), StorefrontProviderError> {
        let mut store = self.store.lock().await;

        self.storefront
            .thrustr_storefront_storefront_provider()
            .call_init(&mut *store)
            .await
            .unwrap()
    }
}
