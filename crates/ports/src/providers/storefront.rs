use crate::metadata::Metadata;
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub enum StorefrontProviderError {
    NotAutorized(String),
    Other(String),
}

pub enum StorefrontProviderStatus {
    Active,
    Inactive,
    Error(StorefrontProviderError),
}

#[async_trait]
pub trait StorefrontProvider: Metadata + Send + Sync {
    fn status(&self) -> StorefrontProviderStatus;
    async fn init(&self) -> Result<(), StorefrontProviderError>;
}
