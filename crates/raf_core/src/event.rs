//! Event bus - lightweight pub/sub system for decoupled communication.
//!
//! Modules publish events (e.g. "entity_selected", "asset_imported")
//! and other modules subscribe to react without direct coupling.

use serde::{Deserialize, Serialize};
use std::any::Any;
use std::collections::HashMap;
use uuid::Uuid;

/// Unique identifier for an event subscription.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EventId(pub Uuid);

impl EventId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for EventId {
    fn default() -> Self {
        Self::new()
    }
}

/// A type-erased event wrapper for the event bus.
struct EventEntry {
    data: Box<dyn Any + Send + Sync>,
}

/// Simple event bus using string-keyed channels.
///
/// For the MVP, events are collected during a frame and drained by subscribers
/// at the end of the frame. This keeps things simple and allocation-light.
pub struct EventBus {
    /// Events posted this frame, keyed by event name.
    events: HashMap<String, Vec<EventEntry>>,
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            events: HashMap::new(),
        }
    }

    /// Publish an event with a string key and typed payload.
    pub fn publish<T: Any + Send + Sync>(&mut self, key: &str, data: T) {
        self.events
            .entry(key.to_string())
            .or_default()
            .push(EventEntry {
                data: Box::new(data),
            });
    }

    /// Drain and read all events of a given key and type. Consumes them.
    pub fn drain<T: Any + Send + Sync>(&mut self, key: &str) -> Vec<T> {
        let entries = match self.events.remove(key) {
            Some(e) => e,
            None => return Vec::new(),
        };

        entries
            .into_iter()
            .filter_map(|entry| entry.data.downcast::<T>().ok().map(|b| *b))
            .collect()
    }

    /// Check if there are any pending events for a key.
    pub fn has_events(&self, key: &str) -> bool {
        self.events
            .get(key)
            .map_or(false, |v| !v.is_empty())
    }

    /// Clear all events (call at end of frame).
    pub fn clear(&mut self) {
        self.events.clear();
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publish_and_drain() {
        let mut bus = EventBus::new();
        bus.publish("entity_selected", 42u32);
        bus.publish("entity_selected", 99u32);

        let events: Vec<u32> = bus.drain("entity_selected");
        assert_eq!(events, vec![42, 99]);
        assert!(!bus.has_events("entity_selected"));
    }

    #[test]
    fn drain_empty() {
        let mut bus = EventBus::new();
        let events: Vec<u32> = bus.drain("nonexistent");
        assert!(events.is_empty());
    }
}
