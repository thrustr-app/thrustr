use futures::FutureExt;
use gpui::{
    App, AppContext, Asset, AssetLogger, Context, Entity, ImageAssetLoader, ImageCache,
    ImageCacheError, ImageCacheItem, ImageCacheProvider, RenderImage, Resource, WeakEntity, Window,
    hash,
};
use lru::LruCache;
use std::{num::NonZeroUsize, sync::Arc};

pub fn lru_image_cache(cache: Entity<LruImageCache>, max_items: usize) -> LruImageCacheProvider {
    LruImageCacheProvider { cache, max_items }
}

pub struct LruImageCacheProvider {
    cache: Entity<LruImageCache>,
    max_items: usize,
}

impl ImageCacheProvider for LruImageCacheProvider {
    fn provide(&mut self, window: &mut Window, cx: &mut App) -> gpui::AnyImageCache {
        if self.cache.read(cx).cap() != self.max_items {
            let max_items = self.max_items;
            self.cache
                .update(cx, |cache, cx| cache.resize(max_items, window, cx));
        }
        self.cache.clone().into()
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

    pub fn remove(&mut self, resource: &Resource, cx: &mut Context<Self>) {
        let key = hash(resource);
        if let Some(mut item) = self.cache.pop(&key)
            && let Some(Ok(image)) = item.get()
        {
            cx.drop_image(image, None);
        }
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
        if let Some(item) = self.cache.get_mut(&key) {
            return item.get();
        }
        self.start_load(key, resource, window, cx);
        None
    }
}
