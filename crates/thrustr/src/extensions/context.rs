use gpui::{AppContext, Context, Task};
use gpui_tokio::Tokio;
use std::future::Future;

/// Extension trait for common task spawning operations.
pub trait SpawnTaskExt<T: 'static> {
    /// Spawns a future in the background and updates the entity with the result.
    /// Calls `notify()` after the handler is called.
    fn spawn_and_update<F, V>(
        &mut self,
        future: F,
        handler: impl Fn(&mut T, V, &mut Context<T>) + Send + 'static,
    ) where
        F: Future<Output = V> + Send + 'static,
        V: Send + 'static;

    /// Spawns a future in the background using Tokio and updates the entity with the result.
    /// Calls `notify()` after the handler is called.
    fn spawn_and_update_tokio<F, V>(
        &mut self,
        future: F,
        handler: impl Fn(&mut T, V, &mut Context<T>) + Send + 'static,
    ) where
        F: Future<Output = V> + Send + 'static,
        V: Send + 'static;
}

impl<'a, T: 'static> SpawnTaskExt<T> for Context<'a, T> {
    fn spawn_and_update<F, V>(
        &mut self,
        future: F,
        handler: impl Fn(&mut T, V, &mut Context<T>) + Send + 'static,
    ) where
        F: Future<Output = V> + Send + 'static,
        V: Send + 'static,
    {
        let task = self.background_spawn(future);
        self.spawn(async move |entity, cx| {
            let result = task.await;
            let _ = entity.update(cx, |entity, cx| {
                handler(entity, result, cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn spawn_and_update_tokio<F, V>(
        &mut self,
        future: F,
        handler: impl Fn(&mut T, V, &mut Context<T>) + Send + 'static,
    ) where
        F: Future<Output = V> + Send + 'static,
        V: Send + 'static,
    {
        let task = Tokio::spawn(self, future);
        self.spawn(async move |entity, cx| {
            let result = task.await.unwrap();
            let _ = entity.update(cx, |entity, cx| {
                handler(entity, result, cx);
                cx.notify();
            });
        })
        .detach();
    }
}

pub trait EventListenerExt<T: 'static> {
    fn listen(
        &mut self,
        event: &'static str,
        handler: impl Fn(&mut T, &mut Context<T>) + Send + 'static,
    ) -> Task<()>;
}

impl<'a, T: 'static> EventListenerExt<T> for Context<'a, T> {
    fn listen(
        &mut self,
        event: &'static str,
        handler: impl Fn(&mut T, &mut Context<T>) + Send + 'static,
    ) -> Task<()> {
        let mut receiver = event::listen(event);
        self.spawn(async move |entity, cx| {
            while let Ok(_) = receiver.recv().await {
                let _ = entity.update(cx, &handler);
            }
        })
    }
}
