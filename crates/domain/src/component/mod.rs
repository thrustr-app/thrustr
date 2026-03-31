use crate::component::capabilities::Storefront;
use async_trait::async_trait;
use std::sync::Arc;

pub mod capabilities;
mod config;
mod error;
mod storage;
mod value_objects;

pub use capabilities::Capability;
pub use config::*;
pub use error::*;
pub use storage::*;
pub use value_objects::*;

/// A component is a unit of functionality provided by the core application or by a plugin.
/// A component may expose one or more capabilities.
#[async_trait]
pub trait Component: Send + Sync {
    fn metadata(&self) -> &Metadata;
    fn status(&self) -> Status;
    fn set_status(&self, status: Status);
    fn config(&self) -> Option<&Config> {
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
