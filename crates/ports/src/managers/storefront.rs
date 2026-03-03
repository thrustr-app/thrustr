use async_trait::async_trait;

use std::sync::Arc;

use crate::capabilities::Storefront;

#[async_trait]
pub trait StorefrontManager: Send + Sync {
    async fn register_storefront(&self, storefront: Arc<dyn Storefront>);
    fn storefronts(&self) -> Vec<Arc<dyn Storefront>>;
    fn storefront(&self, id: &str) -> Option<Arc<dyn Storefront>>;
}
