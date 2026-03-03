use crate::manifest::Manifest;
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub enum StorefrontError {
    NotAutorized(String),
    Other(String),
}

#[derive(Debug, Clone)]
pub enum StorefrontStatus {
    Initializing,
    Active,
    Inactive,
    Error(StorefrontError),
}

impl StorefrontStatus {
    pub fn is_initializing(&self) -> bool {
        matches!(self, StorefrontStatus::Initializing)
    }

    pub fn is_active(&self) -> bool {
        matches!(self, StorefrontStatus::Active)
    }

    pub fn is_inactive(&self) -> bool {
        matches!(self, StorefrontStatus::Inactive)
    }

    pub fn is_error(&self) -> bool {
        matches!(self, StorefrontStatus::Error(_))
    }
}

#[async_trait]
pub trait Storefront: Manifest + Send + Sync {
    fn status(&self) -> StorefrontStatus;
    async fn init(&self) -> Result<(), StorefrontError>;
}
