use crate::component::Component;
use std::sync::{Arc, Weak};

pub mod storefront;

/// A capability represents a specific functionality exposed by a component.
pub trait Capability: Send + Sync {
    /// Returns the component that exposes this capability.
    fn component(self: Arc<Self>) -> Weak<dyn Component>;
}
