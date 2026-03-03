use async_trait::async_trait;
use crate::capabilities::Component;

#[derive(Debug, Clone)]
pub enum StorefrontError {
    NotAutorized(String),
    Other(String),
}

#[async_trait]
pub trait Storefront: Component + Send + Sync {}
