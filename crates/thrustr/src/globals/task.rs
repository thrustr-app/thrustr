use gpui::Context;
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
