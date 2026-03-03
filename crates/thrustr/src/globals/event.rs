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
        let mut receiver = event::listen(event);
        self.spawn(async move |entity, cx| {
            while let Ok(_) = receiver.recv().await {
                let _ = entity.update(cx, &handler);
            }
        })
    }
}
