use futures::FutureExt;
use gpui::{
    App, AppContext, Asset, AssetLogger, Context, ElementId, Entity, ImageAssetLoader, ImageCache,
    ImageCacheError, ImageCacheItem, ImageCacheProvider, RenderImage, Resource, WeakEntity, Window,
    hash,
};
use lru::LruCache;
use std::{
    collections::HashMap,
    num::NonZeroUsize,
    sync::Arc,
    time::{Duration, Instant},
};

const DEFAULT_RETRY_DELAY: Duration = Duration::from_secs(5);

pub fn lru_image_cache(id: impl Into<ElementId>, max_items: usize) -> LruImageCacheProvider {
    LruImageCacheProvider {
        id: id.into(),
        max_items,
        retry_delay: DEFAULT_RETRY_DELAY,
    }
}

pub struct LruImageCacheProvider {
    id: ElementId,
    max_items: usize,
    retry_delay: Duration,
}

impl ImageCacheProvider for LruImageCacheProvider {
    fn provide(&mut self, window: &mut Window, cx: &mut App) -> gpui::AnyImageCache {
        let retry_delay = self.retry_delay;
        let max_items = self.max_items;
        window
            .with_global_id(self.id.clone(), |global_id, window| {
                window.with_element_state::<Entity<LruImageCache>, _>(global_id, |cache, window| {
                    let cache = cache.unwrap_or_else(|| {
                        cx.new(|cx| LruImageCache::new(max_items, retry_delay, cx))
                    });
                    if cache.read(cx).cap() != max_items {
                        cache.update(cx, |c, cx| c.resize(max_items, window, cx));
                    }
                    (cache.clone(), cache)
                })
            })
            .into()
    }
}

struct PendingRetry {
    resource: Resource,
    retry_at: Instant,
}

pub struct LruImageCache {
    retry_delay: Duration,
    cache: LruCache<u64, ImageCacheItem>,
    pending_retry: HashMap<u64, PendingRetry>,
    next_wakeup: Option<Instant>,
    weak_self: WeakEntity<Self>,
}

impl LruImageCache {
    pub fn new(max_items: usize, retry_delay: Duration, cx: &mut Context<Self>) -> Self {
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
            retry_delay,
            cache: LruCache::new(cap),
            pending_retry: HashMap::new(),
            next_wakeup: None,
            weak_self: cx.entity().downgrade(),
        }
    }

    pub fn cap(&self) -> usize {
        self.cache.cap().get()
    }

    fn resize(&mut self, new_cap: usize, window: &mut Window, cx: &mut Context<Self>) {
        let new_cap = new_cap.max(1);
        while self.cache.len() > new_cap {
            if let Some((key, mut evicted)) = self.cache.pop_lru() {
                self.pending_retry.remove(&key);
                if let Some(Ok(image)) = evicted.get() {
                    cx.drop_image(image, Some(window));
                }
            }
        }
        self.cache.resize(NonZeroUsize::new(new_cap).unwrap());
    }

    fn evict_lru_if_full(&mut self, window: &mut Window, cx: &mut App) {
        if self.cache.len() < self.cap() {
            return;
        }
        if let Some((_, mut evicted)) = self.cache.pop_lru()
            && let Some(Ok(image)) = evicted.get()
        {
            cx.drop_image(image, Some(window));
        }
    }

    fn start_load(&mut self, key: u64, resource: &Resource, window: &mut Window, cx: &mut App) {
        let fut = AssetLogger::<ImageAssetLoader>::load(resource.clone(), cx);
        let task = cx.background_executor().spawn(fut).shared();

        self.evict_lru_if_full(window, cx);
        self.cache.put(key, ImageCacheItem::Loading(task.clone()));

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

    fn maybe_schedule_wakeup(&mut self, retry_at: Instant, window: &mut Window, cx: &mut App) {
        let now = Instant::now();

        if self.next_wakeup.is_some_and(|w| w <= now) {
            self.next_wakeup = None;
        }

        if self.next_wakeup.is_some_and(|w| w <= retry_at) {
            return;
        }

        self.next_wakeup = Some(retry_at);
        let delay = retry_at.saturating_duration_since(now);

        let entity = window.current_view();
        window
            .spawn(cx, async move |cx| {
                cx.background_executor().timer(delay).await;
                cx.on_next_frame(move |_, cx| cx.notify(entity));
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

                Some(Err(ImageCacheError::Io(_))) => {
                    self.cache.pop(&key);
                    let retry_at = Instant::now() + self.retry_delay;
                    self.pending_retry.insert(
                        key,
                        PendingRetry {
                            resource: resource.clone(),
                            retry_at,
                        },
                    );
                    self.maybe_schedule_wakeup(retry_at, window, cx);
                    None
                }

                Some(Err(e)) => {
                    let _ = self.cache.get(&key);
                    Some(Err(e))
                }
            };
        }

        if let Some(pending) = self.pending_retry.get(&key) {
            if Instant::now() < pending.retry_at {
                return None;
            }
            let pending = self.pending_retry.remove(&key).unwrap();
            if self.pending_retry.is_empty() {
                self.next_wakeup = None;
            }
            self.start_load(key, &pending.resource, window, cx);
            return None;
        }

        self.start_load(key, resource, window, cx);
        None
    }
}
