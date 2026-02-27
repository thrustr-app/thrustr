use crate::metadata::Metadata;
use async_trait::async_trait;

#[derive(Debug)]
pub enum StorefrontProviderError {
    NotAutorized(String),
    Other(String),
}

#[async_trait]
pub trait StorefrontProvider: Metadata + Send + Sync {
    async fn init(&self) -> Result<(), StorefrontProviderError>;
}
