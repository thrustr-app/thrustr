use async_trait::async_trait;

#[derive(Debug)]
pub struct StorefrontMetadata {
    pub id: String,
    pub name: String,
}

#[derive(Debug)]
pub enum StorefrontProviderError {
    NotAutorized(String),
    Other(String),
}

#[async_trait]
pub trait StorefrontProvider: Send + Sync {
    fn metadata(&self) -> StorefrontMetadata;
    async fn init(&self) -> Result<(), StorefrontProviderError>;
}
