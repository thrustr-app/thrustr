use gpui::{Context, Task};
use gpui_tokio::Tokio;
use std::future::Future;

pub trait SpawnTaskExt<T: 'static> {
    fn spawn_and_update_tokio<F, V, E>(
        &mut self,
        future: F,
        handler: impl Fn(&mut T, Result<V, E>, &mut Context<T>) + Send + 'static,
    ) where
        F: Future<Output = Result<V, E>> + Send + 'static,
        V: Send + 'static,
        E: Send + 'static;
}

impl<'a, T: 'static> SpawnTaskExt<T> for Context<'a, T> {
    fn spawn_and_update_tokio<F, V, E>(
        &mut self,
        future: F,
        handler: impl Fn(&mut T, Result<V, E>, &mut Context<T>) + Send + 'static,
    ) where
        F: Future<Output = Result<V, E>> + Send + 'static,
        V: Send + 'static,
        E: Send + 'static,
    {
        let task = Tokio::spawn(self, future);
        self.spawn(async move |entity, cx| {
            let result = task.await.unwrap();
            let _ = entity.update(cx, |entity, cx| handler(entity, result, cx));
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
