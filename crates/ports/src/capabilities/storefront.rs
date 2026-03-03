use crate::capabilities::Capability;
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub enum StorefrontError {
    NotAutorized(String),
    Other(String),
}

#[async_trait]
pub trait Storefront: Capability {}
