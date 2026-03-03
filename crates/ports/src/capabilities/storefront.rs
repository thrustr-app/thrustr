use crate::capabilities::CapabilityProvider;
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub enum StorefrontError {
    NotAutorized(String),
    Other(String),
}

#[async_trait]
pub trait Storefront: CapabilityProvider + Send + Sync {}
