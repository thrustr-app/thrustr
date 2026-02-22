use crate::providers::StorefrontProvider;
use std::sync::Arc;

pub trait StorefrontManager {
    fn register_storefront(&self, storefront: Arc<dyn StorefrontProvider>);
    fn storefronts(&self) -> Vec<Arc<dyn StorefrontProvider>>;
    fn storefront(&self, id: &str) -> Option<Arc<dyn StorefrontProvider>>;
}
