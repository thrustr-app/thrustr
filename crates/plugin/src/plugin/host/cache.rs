use crate::{plugin::PluginState, wit::thrustr::plugin::cache::Host as CacheHost};
use lru::LruCache;
use runtime::clock::Moment;
use std::{
    sync::{Mutex, MutexGuard, PoisonError},
    time::Duration,
};

const MAX_BYTES: usize = 64 * 1024 * 1024;

pub struct PluginCache {
    entries: Mutex<Entries>,
    max_bytes: usize,
}

struct Entries {
    lru: LruCache<String, Entry>,
    bytes: usize,
}

struct Entry {
    value: Vec<u8>,
    expires_at: Option<Moment>,
}

impl PluginCache {
    pub fn new() -> Self {
        Self::with_max_bytes(MAX_BYTES)
    }

    fn with_max_bytes(max_bytes: usize) -> Self {
        Self {
            entries: Mutex::new(Entries {
                lru: LruCache::unbounded(),
                bytes: 0,
            }),
            max_bytes,
        }
    }

    fn get(&self, key: &str, now: Moment) -> Option<Vec<u8>> {
        let mut entries = self.lock();
        match entries.lru.get(key) {
            Some(entry) if !entry.is_expired(now) => return Some(entry.value.clone()),
            Some(_) => entries.remove(key),
            None => {}
        }
        None
    }

    fn set(&self, key: String, value: Vec<u8>, ttl: Option<Duration>, now: Moment) {
        let mut entries = self.lock();
        entries.remove(&key);

        let size = key.len() + value.len();
        if size > self.max_bytes {
            return;
        }

        entries.bytes += size;
        let entry = Entry {
            value,
            expires_at: ttl.and_then(|ttl| now.checked_add(ttl)),
        };
        entries.lru.push(key, entry);

        if entries.bytes > self.max_bytes {
            entries.evict(self.max_bytes, now);
        }
    }

    fn delete(&self, key: &str) {
        self.lock().remove(key);
    }

    fn list(&self, prefix: Option<&str>, now: Moment) -> Vec<String> {
        let mut keys: Vec<_> = self
            .lock()
            .lru
            .iter()
            .filter(|(key, entry)| {
                !entry.is_expired(now) && prefix.is_none_or(|prefix| key.starts_with(prefix))
            })
            .map(|(key, _)| key.clone())
            .collect();
        keys.sort_unstable();
        keys
    }

    fn lock(&self) -> MutexGuard<'_, Entries> {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Entries {
    fn remove(&mut self, key: &str) {
        if let Some((key, entry)) = self.lru.pop_entry(key) {
            self.bytes -= key.len() + entry.value.len();
        }
    }

    fn evict(&mut self, max_bytes: usize, now: Moment) {
        let expired: Vec<_> = self
            .lru
            .iter()
            .filter(|(_, entry)| entry.is_expired(now))
            .map(|(key, _)| key.clone())
            .collect();
        for key in expired {
            self.remove(&key);
        }

        while self.bytes > max_bytes {
            let Some((key, entry)) = self.lru.pop_lru() else {
                break;
            };
            self.bytes -= key.len() + entry.value.len();
        }
    }
}

impl Entry {
    fn is_expired(&self, now: Moment) -> bool {
        self.expires_at
            .is_some_and(|expires_at| expires_at.has_passed(now))
    }
}

impl CacheHost for PluginState {
    fn get(&mut self, key: String) -> Option<Vec<u8>> {
        self.cache.get(&key, Moment::now())
    }

    fn set(&mut self, key: String, value: Vec<u8>, ttl_ms: Option<u64>) {
        self.cache
            .set(key, value, ttl_ms.map(Duration::from_millis), Moment::now());
    }

    fn delete(&mut self, key: String) {
        self.cache.delete(&key);
    }

    fn list(&mut self, prefix: Option<String>) -> Vec<String> {
        self.cache.list(prefix.as_deref(), Moment::now())
    }
}
