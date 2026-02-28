use crate::metadata::Metadata;
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub enum StorefrontProviderError {
    NotAutorized(String),
    Other(String),
}

#[derive(Debug, Clone)]
pub enum StorefrontProviderStatus {
    Initializing,
    Active,
    Inactive,
    Error(StorefrontProviderError),
}

impl StorefrontProviderStatus {
    pub fn is_initializing(&self) -> bool {
        matches!(self, StorefrontProviderStatus::Initializing)
    }

    pub fn is_active(&self) -> bool {
        matches!(self, StorefrontProviderStatus::Active)
    }

    pub fn is_inactive(&self) -> bool {
        matches!(self, StorefrontProviderStatus::Inactive)
    }

    pub fn is_error(&self) -> bool {
        matches!(self, StorefrontProviderStatus::Error(_))
    }
}

#[async_trait]
pub trait StorefrontProvider: Metadata + Send + Sync {
    fn status(&self) -> StorefrontProviderStatus;
    async fn init(&self) -> Result<(), StorefrontProviderError>;
}
