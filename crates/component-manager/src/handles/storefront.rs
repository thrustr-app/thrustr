use crate::ComponentHandle;
use domain::{capabilities::Storefront, storage::ComponentStorage};
use std::sync::Arc;

#[derive(Clone)]
pub struct StorefrontHandle {
    storefront: Arc<dyn Storefront>,
    storage: Arc<dyn ComponentStorage>,
}

impl StorefrontHandle {
    pub fn new(storefront: Arc<dyn Storefront>, storage: Arc<dyn ComponentStorage>) -> Self {
        Self {
            storefront,
            storage,
        }
    }

    pub fn component(&self) -> ComponentHandle {
        ComponentHandle::new(
            self.storefront.clone().component(),
            Arc::clone(&self.storage),
        )
    }
}
