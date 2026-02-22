use crate::providers::StorefrontProvider;
use std::sync::Arc;

pub trait StorefrontManager: Send + Sync {
    fn register_storefront_provider(&self, storefront: Arc<dyn StorefrontProvider>);
    fn storefront_providers(&self) -> Vec<Arc<dyn StorefrontProvider>>;
    fn storefront_provider(&self, id: &str) -> Option<Arc<dyn StorefrontProvider>>;
}
