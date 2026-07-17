use crate::component::capabilities::Storefront;
use async_trait::async_trait;
use std::sync::Arc;

mod auth;
pub mod capabilities;
mod config;
mod error;
mod image;
mod metadata;
mod status;
mod storage;

pub use auth::*;
pub use config::*;
pub use error::*;
pub use image::*;
pub use metadata::*;
pub use status::*;
pub use storage::*;

/// A component is a unit of functionality provided by the core application or by a plugin.
/// A component may expose one or more capabilities.
#[async_trait]
pub trait Component: Send + Sync {
    fn metadata(&self) -> Metadata<'_>;

    fn config(&self) -> Option<ComponentConfig> {
        None
    }

    /// Returns a storefront capability instance if this component exposes one.
    fn storefront(self: Arc<Self>) -> Option<Arc<dyn Storefront>> {
        None
    }

    async fn init(&self) -> Result<(), Error>;

    async fn get_login_method(&self) -> Result<Option<LoginMethod>, Error>;

    async fn get_logout_flow(&self) -> Result<Option<AuthFlow>, Error>;

    async fn login(
        &self,
        url: Option<String>,
        body: Option<String>,
        fields: Option<Vec<(String, String)>>,
    ) -> Result<(), Error>;

    async fn logout(&self) -> Result<(), Error>;

    async fn validate_config(&self, fields: &[(String, String)]) -> Result<(), Error>;
}
