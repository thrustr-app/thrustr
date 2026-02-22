use async_trait::async_trait;

#[derive(Debug)]
pub enum StorefrontProviderError {
    NotAutorized(String),
    Other(String),
}

#[async_trait]
pub trait StorefrontProvider: Send + Sync {
    fn id(&self) -> &str;
    async fn init(&self) -> Result<(), StorefrontProviderError>;
}
