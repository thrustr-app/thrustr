use std::future::Future;
use tokio::runtime::Handle;
use tokio::task::JoinHandle;

#[derive(Clone)]
pub struct TokioHandle(Handle);

impl TokioHandle {
    pub fn new(handle: Handle) -> Self {
        Self(handle)
    }

    pub fn current() -> Self {
        Self(Handle::current())
    }

    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.0.spawn(future)
    }

    pub fn spawn_blocking<F, R>(&self, f: F) -> JoinHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        self.0.spawn_blocking(f)
    }

    pub fn raw(&self) -> &Handle {
        &self.0
    }
}

impl From<Handle> for TokioHandle {
    fn from(handle: Handle) -> Self {
        Self(handle)
    }
}
