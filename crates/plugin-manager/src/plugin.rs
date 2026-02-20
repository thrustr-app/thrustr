use crate::{
    Storefront,
    exports::thrustr::storefront::storefront_provider::Error as StorefrontProviderError,
    thrustr::storefront::{
        config::{Error as ConfigError, Host as ConfigHost},
        kv_store::{Error as KvStoreError, Host as KvStoreHost},
    },
};
use anyhow::Result;
use ports::PluginStorage;
use std::sync::Arc;
use wasmtime::{Store, component::HasData};
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxView, WasiView};
use wasmtime_wasi_http::{WasiHttpCtx, WasiHttpView};
use xutex::AsyncMutex;

use semver::Version;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct PluginManifest {
    pub plugin: PluginInfo,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub authors: Vec<String>,
    pub version: Version,
    pub description: Option<String>,
}

pub struct PluginState {
    ctx: WasiCtx,
    http_ctx: WasiHttpCtx,
    table: ResourceTable,
    id: String,
    storage: Arc<dyn PluginStorage>,
}

impl PluginState {
    pub fn new(id: &str, storage: Arc<dyn PluginStorage>) -> Self {
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

impl KvStoreHost for PluginState {
    async fn get(&mut self, key: String) -> Result<Option<Vec<u8>>, KvStoreError> {
        self.storage
            .get_data(&self.id, &key)
            .map_err(|e| KvStoreError::Internal(e.to_string()))
    }

    async fn set(&mut self, key: String, value: Vec<u8>) -> Result<(), KvStoreError> {
        self.storage
            .set_data(&self.id, &key, value)
            .map_err(|e| KvStoreError::Internal(e.to_string()))
    }

    async fn delete(&mut self, key: String) -> Result<(), KvStoreError> {
        self.storage
            .delete_data(&self.id, &key)
            .map_err(|e| KvStoreError::Internal(e.to_string()))
    }

    async fn list(&mut self, prefix: Option<String>) -> Result<Vec<String>, KvStoreError> {
        self.storage
            .list_data(&self.id, prefix.as_deref())
            .map_err(|e| KvStoreError::Internal(e.to_string()))
    }
}

impl ConfigHost for PluginState {
    async fn get(&mut self, field_id: String) -> Result<String, ConfigError> {
        self.storage
            .get_config(&self.id, &field_id)
            .map(|v| v.unwrap_or_default())
            .map_err(|e| ConfigError::Internal(e.to_string()))
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

impl WasiHttpView for PluginState {
    fn ctx(&mut self) -> &mut WasiHttpCtx {
        &mut self.http_ctx
    }

    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
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

    pub async fn auth(&self, url: &str, body: &[u8]) -> Result<(), StorefrontProviderError> {
        let mut store = self.store.lock().await;

        self.storefront
            .thrustr_storefront_storefront_provider()
            .call_auth(&mut *store, url, body)
            .await
            .unwrap()
    }
}
