use crate::component::Component;
use std::sync::Arc;

mod storefront;

pub use storefront::*;

/// A capability represents a specific functionality exposed by a component.
pub trait Capability: Send + Sync {
    /// Returns the component that exposes this capability.
    fn component(self: Arc<Self>) -> Arc<dyn Component>;
}
