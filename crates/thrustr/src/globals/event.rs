use gpui::{Context, Task};

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
        self.spawn(async move |entity, cx| {
            let mut listener = event::listen(event);
            while let Ok(_) = listener.recv().await {
                let _ = entity.update(cx, &handler);
            }
        })
    }
}
