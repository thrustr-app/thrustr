use crate::{
    exports::thrustr::plugin::storefront_provider::Error as WasmStorefrontProviderError,
    plugin::Plugin,
};
use async_trait::async_trait;
use ports::{
    managers::Plugin as PluginTrait,
    providers::{StorefrontMetadata, StorefrontProvider, StorefrontProviderError},
};

impl From<WasmStorefrontProviderError> for StorefrontProviderError {
    fn from(e: WasmStorefrontProviderError) -> Self {
        match e {
            WasmStorefrontProviderError::NotAutorized(msg) => {
                StorefrontProviderError::NotAutorized(msg)
            }
            WasmStorefrontProviderError::Other(msg) => StorefrontProviderError::Other(msg),
        }
    }
}

#[async_trait]
impl StorefrontProvider for Plugin {
    fn metadata(&self) -> StorefrontMetadata {
        StorefrontMetadata {
            id: self.id().to_string(),
            name: self.name().to_string(),
        }
    }

    async fn init(&self) -> Result<(), StorefrontProviderError> {
        let (instance, mut store) = self.instantiate_storefront().await?;

        instance
            .thrustr_plugin_storefront_provider()
            .call_init(&mut store)
            .await
            .map_err(|e| StorefrontProviderError::Other(format!("Wasm call failed: {e}")))?
            .map_err(Into::into)
    }
}
