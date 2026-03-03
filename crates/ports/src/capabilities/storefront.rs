use crate::capabilities::Component;
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub enum StorefrontError {
    NotAutorized(String),
    Other(String),
}

#[async_trait]
pub trait Storefront: Component + Send + Sync {}
