use async_trait::async_trait;

use crate::providers::StorefrontProvider;
use std::sync::Arc;

#[async_trait]
pub trait StorefrontManager: Send + Sync {
    async fn register_storefront_provider(&self, storefront: Arc<dyn StorefrontProvider>);
    fn storefront_providers(&self) -> Vec<Arc<dyn StorefrontProvider>>;
    fn storefront_provider(&self, id: &str) -> Option<Arc<dyn StorefrontProvider>>;
}
