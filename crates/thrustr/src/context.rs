use event::Topic;
use futures::future;
use gpui::{AppContext, Context, Task, Window};
use std::future::Future;

/// Extension trait for common task spawning operations.
pub trait SpawnTaskExt<T: 'static> {
    /// Spawns a future in the background and updates the entity with the given handler.
    /// Calls `notify()` after the handler is called.
    fn spawn_and_update<F, V>(
        &mut self,
        future: F,
        handler: impl FnOnce(&mut T, V, &mut Context<T>) + 'static,
    ) -> Task<()>
    where
        F: Future<Output = V> + Send + 'static,
        V: Send + 'static;

    /// Like [`SpawnTaskExt::spawn_and_update`], but the handler also receives the window.
    fn spawn_and_update_in<F, V>(
        &mut self,
        window: &Window,
        future: F,
        handler: impl FnOnce(&mut T, V, &mut Window, &mut Context<T>) + 'static,
    ) -> Task<()>
    where
        F: Future<Output = V> + Send + 'static,
        V: Send + 'static;
}

impl<'a, T: 'static> SpawnTaskExt<T> for Context<'a, T> {
    fn spawn_and_update<F, V>(
        &mut self,
        future: F,
        handler: impl FnOnce(&mut T, V, &mut Context<T>) + 'static,
    ) -> Task<()>
    where
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
    }

    fn spawn_and_update_in<F, V>(
        &mut self,
        window: &Window,
        future: F,
        handler: impl FnOnce(&mut T, V, &mut Window, &mut Context<T>) + 'static,
    ) -> Task<()>
    where
        F: Future<Output = V> + Send + 'static,
        V: Send + 'static,
    {
        let task = self.background_spawn(future);
        self.spawn_in(window, async move |entity, cx| {
            let result = task.await;
            let _ = entity.update_in(cx, |entity, window, cx| {
                handler(entity, result, window, cx);
                cx.notify();
            });
        })
    }
}

/// Extension trait for event listener operations.
pub trait EventListenerExt<T: 'static> {
    /// Listens for a topic and updates the entity with the given handler.
    fn listen(
        &mut self,
        topic: Topic,
        handler: impl Fn(&mut T, &mut Context<T>) + 'static,
    ) -> Task<()>;

    /// Listens for any of the given topics and updates the entity with the given handler.
    fn listen_any(
        &mut self,
        topics: &[Topic],
        handler: impl Fn(&mut T, &mut Context<T>) + 'static,
    ) -> Task<()>;
}

impl<'a, T: 'static> EventListenerExt<T> for Context<'a, T> {
    fn listen(
        &mut self,
        topic: Topic,
        handler: impl Fn(&mut T, &mut Context<T>) + 'static,
    ) -> Task<()> {
        self.listen_any(&[topic], handler)
    }

    fn listen_any(
        &mut self,
        topics: &[Topic],
        handler: impl Fn(&mut T, &mut Context<T>) + 'static,
    ) -> Task<()> {
        let mut receivers: Vec<_> = topics.iter().map(|&topic| event::listen(topic)).collect();
        self.spawn(async move |entity, cx| {
            if receivers.is_empty() {
                return;
            }
            loop {
                let changed = receivers.iter_mut().map(|r| Box::pin(r.changed()));
                let (result, ..) = future::select_all(changed).await;
                if result.is_err() {
                    break;
                }
                let _ = entity.update(cx, &handler);
            }
        })
    }
}
