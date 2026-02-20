pub enum StorefrontError {
    NotAutorized(String),
    Other(String),
}

pub trait Storefront: Send + Sync {
    fn init(&self) -> Result<(), StorefrontError>;
}
