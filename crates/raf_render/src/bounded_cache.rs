//! Small bounded LRU cache shared by renderer-side residency tables.
//!
//! The renderer keeps several fixed-capacity conversion caches. Clearing the
//! whole table when one entry does not fit turns a world with more unique
//! assets than the table into a permanent decode/conversion loop. This cache
//! evicts the least-recently-used entry instead, so a larger working set stays
//! stable.

use std::borrow::Borrow;
use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

/// Bounded least-recently-used map with insertion-order eviction.
#[derive(Debug, Clone)]
pub(crate) struct BoundedCache<K, V> {
    capacity: usize,
    entries: HashMap<K, V>,
    order: VecDeque<K>,
}

impl<K, V> BoundedCache<K, V>
where
    K: Eq + Hash + Clone,
{
    pub(crate) fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            capacity,
            entries: HashMap::with_capacity(capacity.min(1_024)),
            order: VecDeque::with_capacity(capacity.min(1_024)),
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
    }

    pub(crate) fn contains_key(&self, key: &K) -> bool {
        self.entries.contains_key(key)
    }

    pub(crate) fn get(&mut self, key: &K) -> Option<&V> {
        if !self.entries.contains_key(key) {
            return None;
        }
        self.touch(key);
        self.entries.get(key)
    }

    /// Borrowed lookup for callers that only have `&str` for a `String` key.
    /// Avoids allocating a temporary owned key on every visible scene node.
    pub(crate) fn get_borrowed<Q>(&mut self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        if !self.entries.contains_key(key) {
            return None;
        }
        if let Some(index) = self.order.iter().position(|candidate| {
            let borrowed: &Q = candidate.borrow();
            borrowed == key
        }) {
            if let Some(candidate) = self.order.remove(index) {
                self.order.push_back(candidate);
            }
        }
        self.entries.get(key)
    }

    pub(crate) fn insert(&mut self, key: K, value: V) {
        self.entries.insert(key.clone(), value);
        self.touch(&key);
        self.evict_to_capacity(Some(&key));
    }

    pub(crate) fn retain(&mut self, mut keep: impl FnMut(&K, &mut V) -> bool) {
        self.entries.retain(|key, value| keep(key, value));
        let entries = &self.entries;
        self.order.retain(|key| entries.contains_key(key));
    }

    fn touch(&mut self, key: &K) {
        self.order.retain(|candidate| candidate != key);
        self.order.push_back(key.clone());
    }

    fn evict_to_capacity(&mut self, protect: Option<&K>) {
        while self.entries.len() > self.capacity {
            let Some(front) = self.order.front().cloned() else {
                break;
            };
            if protect.is_some_and(|key| key == &front) {
                self.order.pop_front();
                self.order.push_back(front);
                continue;
            }
            self.order.pop_front();
            self.entries.remove(&front);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::BoundedCache;

    #[test]
    fn evicts_one_oldest_entry_instead_of_clearing_everything() {
        let mut cache = BoundedCache::new(2);
        cache.insert("a", 1);
        cache.insert("b", 2);
        cache.insert("c", 3);

        assert_eq!(cache.len(), 2);
        assert!(!cache.contains_key(&"a"));
        assert_eq!(cache.get(&"b"), Some(&2));
        assert_eq!(cache.get(&"c"), Some(&3));
    }

    #[test]
    fn get_refreshes_recency() {
        let mut cache = BoundedCache::new(2);
        cache.insert("a", 1);
        cache.insert("b", 2);
        assert_eq!(cache.get(&"a"), Some(&1));
        cache.insert("c", 3);

        assert_eq!(cache.get(&"a"), Some(&1));
        assert!(!cache.contains_key(&"b"));
    }

    #[test]
    fn retain_drops_orphaned_order_entries() {
        let mut cache = BoundedCache::new(4);
        cache.insert("a", 1);
        cache.insert("b", 2);
        cache.retain(|_, value| *value == 2);
        cache.insert("c", 3);
        cache.insert("d", 4);

        assert_eq!(cache.len(), 3);
        assert!(!cache.contains_key(&"a"));
    }
}
