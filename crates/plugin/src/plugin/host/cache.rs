use crate::{plugin::PluginState, wit::thrustr::plugin::cache::Host as CacheHost};
use lru::LruCache;
use runtime::clock::Moment;
use std::{
    sync::{Mutex, MutexGuard, PoisonError},
    time::Duration,
};

const MAX_BYTES: usize = 8 * 1024 * 1024;

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
        let entry = entries.lru.get(key)?;
        if !entry.is_expired(now) {
            return Some(entry.value.clone());
        }
        entries.remove(key);
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

        while entries.bytes > self.max_bytes {
            let Some((key, entry)) = entries.lru.pop_lru() else {
                break;
            };
            entries.bytes -= key.len() + entry.value.len();
        }
    }

    fn delete(&self, key: &str) {
        self.lock().remove(key);
    }

    fn list(&self, prefix: &str, now: Moment) -> Vec<String> {
        let mut keys: Vec<_> = self
            .lock()
            .lru
            .iter()
            .filter(|(key, entry)| !entry.is_expired(now) && key.starts_with(prefix))
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
}

impl Entry {
    fn is_expired(&self, now: Moment) -> bool {
        self.expires_at
            .is_some_and(|expires_at| expires_at.has_passed(now))
    }
}

impl CacheHost for PluginState {
    fn get(&mut self, key: String) -> Option<Vec<u8>> {
        self.runtime.cache.get(&key, Moment::now())
    }

    fn set(&mut self, key: String, value: Vec<u8>, ttl_ms: Option<u64>) {
        self.runtime
            .cache
            .set(key, value, ttl_ms.map(Duration::from_millis), Moment::now());
    }

    fn delete(&mut self, key: String) {
        self.runtime.cache.delete(&key);
    }

    fn list(&mut self, prefix: String) -> Vec<String> {
        self.runtime.cache.list(&prefix, Moment::now())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENTRY_BYTES: usize = 10;
    const TTL: Duration = Duration::from_secs(60);

    #[derive(Clone, Copy)]
    enum Op {
        Set(&'static str, u8),
        SetExpiring(&'static str, u8),
        SetTooLarge(&'static str),
        Get(&'static str),
        Delete(&'static str),
        /// Lets every expiring value expire.
        Wait,
    }
    use Op::*;

    fn value(key: &str, fill: u8) -> Vec<u8> {
        vec![fill; ENTRY_BYTES - key.len()]
    }

    fn run(ops: &[Op]) -> (PluginCache, Moment) {
        let cache = PluginCache::with_max_bytes(3 * ENTRY_BYTES);
        let mut now = Moment::now();
        for &op in ops {
            match op {
                Set(key, fill) => cache.set(key.to_owned(), value(key, fill), None, now),
                SetExpiring(key, fill) => {
                    cache.set(key.to_owned(), value(key, fill), Some(TTL), now);
                }
                SetTooLarge(key) => cache.set(key.to_owned(), vec![0; 4 * ENTRY_BYTES], None, now),
                Get(key) => drop(cache.get(key, now)),
                Delete(key) => cache.delete(key),
                Wait => now = now.checked_add(2 * TTL).unwrap(),
            }
        }
        (cache, now)
    }

    #[track_caller]
    fn check(ops: &[Op], expected: &[(&str, u8)]) {
        let (cache, now) = run(ops);
        let stored: Vec<_> = cache
            .list("", now)
            .into_iter()
            .map(|key| {
                let value = cache.get(&key, now);
                (key, value)
            })
            .collect();
        let expected: Vec<_> = expected
            .iter()
            .map(|&(key, fill)| (key.to_owned(), Some(value(key, fill))))
            .collect();

        assert_eq!(stored, expected);
    }

    #[track_caller]
    fn check_list(ops: &[Op], prefix: &str, expected: &[&str]) {
        let (cache, now) = run(ops);
        assert_eq!(cache.list(prefix, now), expected);
    }

    #[test]
    fn stored_values_can_be_read_back() {
        check(&[Set("a", 1), Set("b", 2)], &[("a", 1), ("b", 2)]);
    }

    #[test]
    fn setting_a_key_again_replaces_its_value() {
        check(&[Set("a", 1), Set("a", 2)], &[("a", 2)]);
    }

    #[test]
    fn deleted_values_are_gone() {
        check(&[Set("a", 1), Set("b", 1), Delete("a")], &[("b", 1)]);
        check(&[Delete("a")], &[]);
    }

    #[test]
    fn values_expire_after_their_ttl() {
        check(&[SetExpiring("a", 1), Set("b", 1)], &[("a", 1), ("b", 1)]);
        check(&[SetExpiring("a", 1), Set("b", 1), Wait], &[("b", 1)]);
    }

    #[test]
    fn a_full_cache_evicts_the_least_recently_used_value() {
        check(
            &[Set("a", 1), Set("b", 1), Set("c", 1), Set("d", 1)],
            &[("b", 1), ("c", 1), ("d", 1)],
        );
        check(
            &[Set("a", 1), Set("b", 1), Set("c", 1), Get("a"), Set("d", 1)],
            &[("a", 1), ("c", 1), ("d", 1)],
        );
    }

    #[test]
    fn replaced_and_deleted_values_free_their_space() {
        check(
            &[Set("a", 1), Set("a", 2), Set("b", 1), Set("c", 1)],
            &[("a", 2), ("b", 1), ("c", 1)],
        );
        check(
            &[
                Set("a", 1),
                Set("b", 1),
                Set("c", 1),
                Delete("a"),
                Set("d", 1),
            ],
            &[("b", 1), ("c", 1), ("d", 1)],
        );
    }

    #[test]
    fn values_larger_than_the_cache_are_not_stored() {
        check(&[Set("a", 1), SetTooLarge("big")], &[("a", 1)]);
        check(&[Set("a", 1), SetTooLarge("a")], &[]);
    }

    #[test]
    fn listing_returns_the_sorted_keys_with_a_prefix() {
        let ops = &[Set("games/2", 1), Set("users/1", 1), Set("games/1", 1)];

        check_list(ops, "games/", &["games/1", "games/2"]);
        check_list(ops, "", &["games/1", "games/2", "users/1"]);
        check_list(ops, "none", &[]);
    }
}
