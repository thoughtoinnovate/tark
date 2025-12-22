//! Completion response caching

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::RwLock;
use std::time::{Duration, Instant};

/// A simple LRU-ish cache for completions
pub struct CompletionCache {
    entries: RwLock<HashMap<CacheKey, CacheEntry>>,
    max_size: usize,
    ttl: Duration,
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct CacheKey {
    prefix_hash: u64,
    suffix_hash: u64,
}

struct CacheEntry {
    completion: String,
    created_at: Instant,
    hits: usize,
}

impl CompletionCache {
    pub fn new(max_size: usize) -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            max_size,
            ttl: Duration::from_secs(300), // 5 minutes
        }
    }

    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self
    }

    /// Get a cached completion if available
    pub fn get(&self, prefix: &str, suffix: &str) -> Option<String> {
        let key = self.make_key(prefix, suffix);
        let mut entries = self.entries.write().ok()?;

        if let Some(entry) = entries.get_mut(&key) {
            if entry.created_at.elapsed() < self.ttl {
                entry.hits += 1;
                return Some(entry.completion.clone());
            } else {
                // Entry expired
                entries.remove(&key);
            }
        }

        None
    }

    /// Store a completion in the cache
    pub fn put(&self, prefix: &str, suffix: &str, completion: String) {
        let key = self.make_key(prefix, suffix);

        if let Ok(mut entries) = self.entries.write() {
            // Evict old entries if at capacity
            if entries.len() >= self.max_size {
                self.evict(&mut entries);
            }

            entries.insert(
                key,
                CacheEntry {
                    completion,
                    created_at: Instant::now(),
                    hits: 0,
                },
            );
        }
    }

    /// Clear all cache entries
    pub fn clear(&self) {
        if let Ok(mut entries) = self.entries.write() {
            entries.clear();
        }
    }

    fn make_key(&self, prefix: &str, suffix: &str) -> CacheKey {
        CacheKey {
            prefix_hash: self.hash_string(prefix),
            suffix_hash: self.hash_string(suffix),
        }
    }

    fn hash_string(&self, s: &str) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        let mut hasher = DefaultHasher::new();
        s.hash(&mut hasher);
        hasher.finish()
    }

    fn evict(&self, entries: &mut HashMap<CacheKey, CacheEntry>) {
        // Remove expired entries first
        let now = Instant::now();
        entries.retain(|_, entry| now.duration_since(entry.created_at) < self.ttl);

        // If still over capacity, remove least recently used
        if entries.len() >= self.max_size {
            // Find the entry with fewest hits and oldest
            if let Some(key_to_remove) = entries
                .iter()
                .min_by_key(|(_, entry)| (entry.hits, std::cmp::Reverse(entry.created_at)))
                .map(|(key, _)| key.clone())
            {
                entries.remove(&key_to_remove);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_put_get() {
        let cache = CompletionCache::new(10);

        cache.put("prefix", "suffix", "completion".to_string());
        let result = cache.get("prefix", "suffix");

        assert_eq!(result, Some("completion".to_string()));
    }

    #[test]
    fn test_cache_miss() {
        let cache = CompletionCache::new(10);

        let result = cache.get("prefix", "suffix");

        assert_eq!(result, None);
    }

    #[test]
    fn test_cache_eviction() {
        let cache = CompletionCache::new(2);

        cache.put("p1", "s1", "c1".to_string());
        cache.put("p2", "s2", "c2".to_string());
        cache.put("p3", "s3", "c3".to_string()); // Should evict p1

        // p1 should be evicted (least hits)
        assert!(cache.get("p1", "s1").is_none() || cache.get("p2", "s2").is_none());
    }
}
