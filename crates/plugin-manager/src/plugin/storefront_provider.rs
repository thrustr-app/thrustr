use crate::{
    exports::thrustr::plugin::storefront_provider::Error as WasmStorefrontProviderError,
    plugin::Plugin,
};
use async_trait::async_trait;
use ports::providers::{StorefrontProvider, StorefrontProviderError, StorefrontProviderStatus};

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
    fn status(&self) -> StorefrontProviderStatus {
        if let Some(error) = &*self.storefront_provider_error.lock() {
            StorefrontProviderStatus::Error(error.clone())
        } else {
            StorefrontProviderStatus::Active
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
            .inspect_err(|e: &StorefrontProviderError| {
                *self.storefront_provider_error.lock() = Some(e.clone())
            })
    }
}
