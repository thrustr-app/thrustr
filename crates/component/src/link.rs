use crate::{ComponentHandle, Timers, handles::WeakComponentHandle};
use std::sync::{Arc, OnceLock};

/// A weak reference to a component's handle.
///
/// Lets a component act on inteself through the host (e.g. sechedule timers
/// tied to its current activation and status).
#[derive(Clone)]
pub struct ComponentLink(Arc<OnceLock<WeakComponentHandle>>);

impl ComponentLink {
    pub(crate) fn new() -> Self {
        Self(Arc::default())
    }

    pub fn timers(&self) -> Timers<'_> {
        Timers::new(self)
    }

    pub(crate) fn bind(&self, handle: &ComponentHandle) {
        self.0
            .set(handle.downgrade())
            .expect("link should be bound by a single registration");
    }

    pub(crate) fn handle(&self) -> Option<ComponentHandle> {
        self.0.get()?.upgrade()
    }
}
