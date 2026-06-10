use futures::FutureExt;
use gpui::{
    App, AppContext, Asset, AssetLogger, Context, ElementId, Entity, ImageAssetLoader, ImageCache,
    ImageCacheError, ImageCacheItem, ImageCacheProvider, RenderImage, Resource, WeakEntity, Window,
    hash,
};
use lru::LruCache;
use std::{num::NonZeroUsize, sync::Arc};

pub fn lru_image_cache(id: impl Into<ElementId>, max_items: usize) -> LruImageCacheProvider {
    LruImageCacheProvider {
        id: id.into(),
        max_items,
    }
}

pub struct LruImageCacheProvider {
    id: ElementId,
    max_items: usize,
}

impl ImageCacheProvider for LruImageCacheProvider {
    fn provide(&mut self, window: &mut Window, cx: &mut App) -> gpui::AnyImageCache {
        let max_items = self.max_items;
        window
            .with_global_id(self.id.clone(), |global_id, window| {
                window.with_element_state::<Entity<LruImageCache>, _>(global_id, |cache, window| {
                    let cache =
                        cache.unwrap_or_else(|| cx.new(|cx| LruImageCache::new(max_items, cx)));
                    if cache.read(cx).cap() != max_items {
                        cache.update(cx, |c, cx| c.resize(max_items, window, cx));
                    }
                    (cache.clone(), cache)
                })
            })
            .into()
    }
}

pub struct LruImageCache {
    cache: LruCache<u64, ImageCacheItem>,
    weak_self: WeakEntity<Self>,
}

impl LruImageCache {
    pub fn new(max_items: usize, cx: &mut Context<Self>) -> Self {
        cx.on_release(|this, cx| {
            while let Some((_, mut item)) = this.cache.pop_lru() {
                if let Some(Ok(image)) = item.get() {
                    cx.drop_image(image, None);
                }
            }
        })
        .detach();

        let cap = NonZeroUsize::new(max_items.max(1)).unwrap();
        Self {
            cache: LruCache::new(cap),
            weak_self: cx.entity().downgrade(),
        }
    }

    pub fn cap(&self) -> usize {
        self.cache.cap().get()
    }

    fn resize(&mut self, new_cap: usize, window: &mut Window, cx: &mut Context<Self>) {
        let new_cap = new_cap.max(1);
        while self.cache.len() > new_cap {
            if let Some((_, mut evicted)) = self.cache.pop_lru()
                && let Some(Ok(image)) = evicted.get()
            {
                cx.drop_image(image, Some(window));
            }
        }
        self.cache.resize(NonZeroUsize::new(new_cap).unwrap());
    }

    fn start_load(&mut self, key: u64, resource: &Resource, window: &mut Window, cx: &mut App) {
        let fut = AssetLogger::<ImageAssetLoader>::load(resource.clone(), cx);
        let task = cx.background_executor().spawn(fut).shared();

        if let Some((_, mut evicted)) = self.cache.push(key, ImageCacheItem::Loading(task.clone()))
            && let Some(Ok(image)) = evicted.get()
        {
            cx.drop_image(image, Some(window));
        }

        let weak = self.weak_self.clone();
        window
            .spawn(cx, async move |cx| {
                _ = task.await;
                cx.on_next_frame(move |_, cx| {
                    if let Some(entity) = weak.upgrade() {
                        cx.update_entity(&entity, |cache, cx| {
                            if cache.cache.contains(&key) {
                                cx.notify();
                            }
                        });
                    }
                });
            })
            .detach();
    }
}

impl ImageCache for LruImageCache {
    fn load(
        &mut self,
        resource: &Resource,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        let key = hash(resource);

        if let Some(result) = self.cache.peek_mut(&key).map(|item| item.get()) {
            return match result {
                None => None,

                Some(Ok(image)) => {
                    let _ = self.cache.get(&key);
                    Some(Ok(image))
                }

                Some(Err(e)) => {
                    let _ = self.cache.get(&key);
                    Some(Err(e))
                }
            };
        }

        self.start_load(key, resource, window, cx);
        None
    }
}
