use event::Topic;
use futures::future;
use gpui::{AppContext, Context, Task, Window};
use std::future::Future;
use tokio::sync::watch::Receiver;

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

    /// Like [`EventListenerExt::listen_any`], but the handler also receives the window.
    fn listen_any_in(
        &mut self,
        window: &Window,
        topics: &[Topic],
        handler: impl Fn(&mut T, &mut Window, &mut Context<T>) + 'static,
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
        let mut receivers = listen_all(topics);
        self.spawn(async move |entity, cx| {
            while next_change(&mut receivers).await {
                let _ = entity.update(cx, &handler);
            }
        })
    }

    fn listen_any_in(
        &mut self,
        window: &Window,
        topics: &[Topic],
        handler: impl Fn(&mut T, &mut Window, &mut Context<T>) + 'static,
    ) -> Task<()> {
        let mut receivers = listen_all(topics);
        self.spawn_in(window, async move |entity, cx| {
            while next_change(&mut receivers).await {
                let _ = entity.update_in(cx, |this, window, cx| handler(this, window, cx));
            }
        })
    }
}

fn listen_all(topics: &[Topic]) -> Vec<Receiver<()>> {
    topics.iter().map(|&topic| event::listen(topic)).collect()
}

/// Waits for any of `receivers` to change, returning `false` once one closes.
async fn next_change(receivers: &mut [Receiver<()>]) -> bool {
    if receivers.is_empty() {
        return false;
    }
    let changed = receivers.iter_mut().map(|r| Box::pin(r.changed()));
    let (result, ..) = future::select_all(changed).await;
    result.is_ok()
}
